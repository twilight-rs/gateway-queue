use http_body_util::Full;
use hyper::{body::Bytes, Response};
use hyper_util::{rt::TokioIo, server::graceful::GracefulShutdown};
use serde::Deserialize;
use std::{
    env,
    error::Error,
    net::{IpAddr, SocketAddr},
    pin::pin,
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::Mutex,
    time::{self, sleep},
};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;
use twilight_gateway_queue::{InMemoryQueue, Queue};
use twilight_http::Client;

const PROCESSED: Bytes = Bytes::from_static(br#"{"message": "You're free to connect now! :)"}"#);

#[cfg(windows)]
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}

#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
    let mut sigterm = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");

    tokio::select! {
        _ = sigint.recv() => {},
        _ = sigterm.recv() => {},
    };
}

#[derive(Deserialize)]
struct QueryParameters {
    shard: u32,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let host_raw = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let host = IpAddr::from_str(&host_raw)?;
    let port = env::var("PORT").unwrap_or_else(|_| "80".into()).parse()?;

    let (client, queue) = {
        if let Ok(token) = env::var("DISCORD_TOKEN") {
            let client = Client::new(token);
            let session = client
                .gateway()
                .authed()
                .await?
                .model()
                .await?
                .session_start_limit;

            let reset_after = Duration::from_millis(session.reset_after);
            (
                Some(Arc::new((
                    client,
                    Mutex::new((time::Instant::now() + reset_after, session.remaining)),
                ))),
                InMemoryQueue::new(
                    session.max_concurrency,
                    session.remaining,
                    reset_after,
                    session.total,
                ),
            )
        } else {
            (None, InMemoryQueue::default())
        }
    };

    let address = SocketAddr::from((host, port));

    let tcp_listener = TcpListener::bind(address).await?;
    let server = hyper::server::conn::http1::Builder::new();
    let graceful = GracefulShutdown::new();
    let mut shutdown = pin!(shutdown_signal());

    info!("Listening on http://{}", address);

    loop {
        tokio::select! {
            conn = tcp_listener.accept() => {
                let Ok((stream, _peer_addr)) = conn else {
                    continue;
                };

                let stream = TokioIo::new(stream);

                let client = client.clone();
                let queue = queue.clone();

                let conn = server.serve_connection(stream, hyper::service::service_fn(move |request| {
                    let queue = queue.clone();

                    let mut shard = None;

                    if client.is_some() {
                        if let Some(query) = request.uri().query() {
                            if let Ok(params) = serde_urlencoded::from_str::<QueryParameters>(query) {
                                shard = Some(params.shard);
                            }
                        }

                        if shard.is_none() {
                            warn!(
                                "No shard id set, defaulting to 0. Will not bucket requests correctly!"
                            );
                        }
                    }
                    let client = client.clone();

                    async move {
                        if let Some((client, lock)) = client.as_deref() {
                            let mut lock = lock.lock().await;
                            if lock.1 > 0 {
                                lock.1 -= 1;
                            } else {
                                time::sleep_until(lock.0).await;
                                'label: {
                                    if let Ok(res) = client.gateway().authed().await {
                                        if let Ok(info) = res.model().await {
                                            let session = info.session_start_limit;
                                            let reset_after =
                                                Duration::from_millis(session.reset_after);
                                            info!("next session start limit in: {reset_after:.2?}");

                                            lock.1 = session.remaining;
                                            lock.0 = time::Instant::now() + reset_after;

                                            queue.update(
                                                session.max_concurrency,
                                                session.remaining,
                                                reset_after,
                                                session.total,
                                            );
                                            break 'label;
                                        }
                                    }

                                    warn!("unable to get new session limits, skipping (this may cause bad things)");
                                }
                            }
                        }

                        queue
                            .enqueue(shard.unwrap_or(0))
                            .await
                            .expect("never cancels");

                        let body = Full::from(PROCESSED);

                        Ok::<Response<Full<Bytes>>, hyper::Error>(Response::new(body))
                    }
                }));

                let conn = graceful.watch(conn);

                tokio::spawn(async move {
                    if let Err(err) = conn.await {
                        error!("Connection error: {}", err);
                    }
                });
            },
            _ = shutdown.as_mut() => {
                drop(tcp_listener);
                info!("Shutdown signal received, initiating shutdown");
                break;
            }
        }
    }

    tokio::select! {
        _ = graceful.shutdown() => {
            info!("Gracefully shutdown!");
        },
        _ = sleep(Duration::from_secs(10)) => {
            error!("Waited 10 seconds for graceful shutdown, aborting...");
        }
    }

    Ok(())
}
