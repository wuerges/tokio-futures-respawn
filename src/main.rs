use std::future::Future;
use std::io;
use std::panic;
use tokio::task;
use tokio::time::sleep;
use tracing::*;
use tracing_subscriber::*;

fn make_future() -> impl Future<Output = Result<i32, io::Error>> {
    async { panic!("boom") }
}

async fn spawn_future_retry_panics<T, F, Fut>(f: F)
where
    T: Send + 'static,
    F: Fn() -> Fut,
    Fut: Future<Output = T> + Send + 'static,
{
    loop {
        let handle = tokio::spawn(f());
        match handle.await {
            Ok(r) => {
                sleep(std::time::Duration::from_secs(1)).await;
            }
            Err(e) => {
                if e.is_panic() {
                    sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    // let join_handle: task::JoinHandle<Result<i32, io::Error>> = tokio::spawn(async {
    //     panic!("boom");
    // });

    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::TRACE)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let join_handle = tokio::spawn(spawn_future_retry_panics(make_future));

    let result = join_handle.await;
    match result {
        Ok(x) => Ok(x),
        Err(err) => {
            sleep(std::time::Duration::from_secs(1)).await;
            Err(err.into())
        }
    }
}
