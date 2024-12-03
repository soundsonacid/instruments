use tokio::runtime::{Handle, Runtime};

pub trait Spawner {
    fn spawn_task<F>(&self, future: F)
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static;
}

impl Spawner for Runtime {
    fn spawn_task<F>(&self, future: F)
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.spawn(future);
    }
}

impl Spawner for Handle {
    fn spawn_task<F>(&self, future: F)
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.spawn(future);
    }
}
