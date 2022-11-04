use std::sync::Arc;
use std::{sync::atomic::AtomicI16};
use std::time::{Duration};
use assignment_1_solution::{Handler, ModuleRef, System};
use ntest::timeout;

#[cfg(test)]
#[ctor::ctor]
fn init() {
    env_logger::init();
}

#[derive(Clone)]
struct Tick;

struct Counter {
    c: Arc<AtomicI16>,
}

impl Counter {
    fn new(c : Arc<AtomicI16>) -> Self {
        Counter {c }
    }
}

#[async_trait::async_trait]
impl Handler<Tick> for Counter {
    async fn handle(&mut self, _self_ref: &ModuleRef<Self> , _msg:Tick) {
        self.c.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }
}

#[tokio::test]
#[timeout(1000)]
async fn timer_stopped_after_shutdown() {
    let mut sys = System::new().await;
    let c = Arc::new(AtomicI16::new(0));
    let m1 = sys.register_module(Counter::new(c.clone())).await;
    let _timer1 = m1.request_tick(Tick, Duration::from_millis(50)).await;
    let _timer2 = m1.request_tick(Tick, Duration::from_millis(50)).await;
    let _timer3 = m1.request_tick(Tick, Duration::from_millis(50)).await;

    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(c.load(std::sync::atomic::Ordering::Acquire), 6);
    sys.shutdown().await;
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(c.load(std::sync::atomic::Ordering::Acquire), 6);
}

#[tokio::test]
#[timeout(1000)]
async fn timer_stopped() {
    let mut sys = System::new().await;
    let c = Arc::new(AtomicI16::new(0));
    let m1 = sys.register_module(Counter::new(c.clone())).await;
    let m2 = m1.clone();
    let m3 = m1.clone();
    let _timer1 = m1.request_tick(Tick, Duration::from_millis(50)).await;
    let _timer2 = m2.request_tick(Tick, Duration::from_millis(50)).await;
    let _timer3 = m3.request_tick(Tick, Duration::from_millis(50)).await;

    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(c.load(std::sync::atomic::Ordering::Acquire), 6);
    _timer1.stop().await;
    _timer2.stop().await;
    _timer3.stop().await;
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(c.load(std::sync::atomic::Ordering::Acquire), 6);
    sys.shutdown().await;
}

#[tokio::test]
#[timeout(1000)]
async fn timer_module_ref_dropped() {
    let mut sys = System::new().await;
    let c = Arc::new(AtomicI16::new(0));
    let m1 = sys.register_module(Counter::new(c.clone())).await;
    let _timer1 = m1.request_tick(Tick, Duration::from_millis(50)).await;
    let _timer2 = m1.request_tick(Tick, Duration::from_millis(50)).await;
    let _timer3 = m1.request_tick(Tick, Duration::from_millis(50)).await;

    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(c.load(std::sync::atomic::Ordering::Acquire), 6);
    std::mem::drop(m1);
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(c.load(std::sync::atomic::Ordering::Acquire), 12);
    sys.shutdown().await;
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(c.load(std::sync::atomic::Ordering::Acquire), 12);
}

#[derive(Clone)]
struct Sleep;

struct Sleeper;

#[async_trait::async_trait]
impl Handler<Sleep> for Sleeper {
    async fn handle(&mut self, _self_ref: &ModuleRef<Self> , _msg:Sleep) {
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[tokio::test]
#[timeout(2500)]
async fn time_sleep() {
    let mut sys = System::new().await;
    let mut sleepers = Vec::new();

    for _ in 0..100 {
        sleepers.push(sys.register_module(Sleeper).await);
    }

    for m in sleepers.iter() {
        m.send(Sleep).await;
    }

    tokio::time::sleep(Duration::from_millis(1)).await;

    sys.shutdown().await;
}