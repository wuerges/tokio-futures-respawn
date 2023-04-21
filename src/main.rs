use std::future::Future;
use std::io;
use std::panic;
use tokio::task;
use tokio::task::JoinHandle;

fn make_future() -> impl Future<Output = Result<i32, io::Error>> {
    async { panic!("boom") }
}

fn spawn_future_retry_panics<T, F, Fut>(f: F) -> JoinHandle<T>
where
    T: Send + 'static,
    F: FnOnce() -> Fut,
    Fut: Future<Output = T> + Send + 'static,
{
    tokio::spawn(f())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    // let join_handle: task::JoinHandle<Result<i32, io::Error>> = tokio::spawn(async {
    //     panic!("boom");
    // });

    let join_handle: task::JoinHandle<_> = spawn_future_retry_panics(make_future);

    let err = join_handle.await.unwrap_err();
    assert!(err.is_panic());
    Ok(())
}
