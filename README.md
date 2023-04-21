# Utility function to respawn failed long running tasks

Create a Task factory that will be used to respawn a new task, if the running one fails.

impl `FutureFactory` to define how the task will be respawned.

impl `ErrorHandler` to customize error handling.

Call `make_future_respawnable` to put your factory to work, with the error handler.

```rust
struct TaskReturnsResult;

impl FutureFactory<Result<(), io::Error>> for TaskReturnsResult {
    fn build_future(&mut self) -> Pin<Box<dyn Future<Output = Result<(), io::Error>> + Send>> {
        Box::pin(async { panic!("boom") })
    }
}

let factory = TaskReturnsResult;

let handler = AlwaysRespawnAndTrace {
    duration: std::time::Duration::from_millis(1),
};

let join_handle = tokio::spawn(make_future_respawnable(handler, factory));

sleep(Duration::from_millis(10)).await;

join_handle.abort();

let err = join_handle.await.unwrap_err();
assert!(!err.is_panic(), "{:?}", err);
assert!(err.is_cancelled(), "{:?}", err);
```
