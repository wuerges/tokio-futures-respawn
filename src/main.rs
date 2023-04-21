use std::fmt::Debug;
use std::future::Future;
use std::io;
use std::panic;
use tokio::time::sleep;
use tracing::*;
use tracing_subscriber::*;

fn make_future() -> impl Future<Output = Result<i32, io::Error>> {
    async { panic!("boom") }
}

async fn sleep_1_second() {
    sleep(std::time::Duration::from_secs(1)).await;
}

async fn make_future_respawnable<T, E, F, Fut>(f: F)
where
    E: std::error::Error + Send + 'static,
    T: Send + 'static + Debug,
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
{
    loop {
        let handle = tokio::spawn(f());
        match handle.await {
            Ok(r) => match r {
                Ok(t) => {
                    info!(?t, "respawnable future finished");
                }
                Err(e) => {
                    info!(?e, "respawnable future finished with error");
                }
            },
            Err(e) => {
                if e.is_panic() {
                    error!(?e, "respawnable future aborted with panic");
                } else {
                    error!(?e, "respawnable future aborted with error");
                }
            }
        }
        sleep_1_second().await;
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::TRACE)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let join_handle = tokio::spawn(make_future_respawnable(make_future));

    Ok(join_handle.await?)
}
