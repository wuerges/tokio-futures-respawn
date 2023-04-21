use std::fmt::Debug;
use std::future::Future;
use std::io;
use std::panic;
use std::pin::Pin;
use tokio::time::sleep;
use tracing::*;
use tracing_subscriber::*;

fn make_future() -> impl Future<Output = Result<i32, io::Error>> {
    async { panic!("boom") }
}

#[derive(Clone, Debug)]
struct Dummy;

// fn make_future_takes_dummy(dummy: Dummy) -> Pin<Box<dyn Future<Output = Result<u32, io::Error>>>> {
//     Box::pin(async move {
//         info!(?dummy, "use dummy for something");
//         panic!("boom")
//     })
// }

fn takes_dummy_makes_make_future(
    dummy: Dummy,
) -> impl Fn() -> Pin<Box<dyn Future<Output = Result<u64, io::Error>> + Send>> + 'static + Send {
    Box::new(|| todo!())
    // async move {
    //     info!(?dummy, "use dummy for something");
    //     panic!("boom")
    // }
}

async fn sleep_1_second() {
    sleep(std::time::Duration::from_secs(1)).await;
}

async fn make_future_respawnable<T, E, F>(f: F)
where
    E: std::error::Error + Send + 'static,
    T: Send + 'static + Debug,
    F: Fn() -> Pin<Box<dyn Future<Output = Result<T, E>> + Send>>,
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

    let dummy = Dummy;

    let make_future = takes_dummy_makes_make_future(dummy);

    let join_handle = tokio::spawn(make_future_respawnable::<u64, io::Error, _>(make_future));
    // let join_handle = tokio::spawn(make_future_respawnable(make_future));

    Ok(join_handle.await?)
}
