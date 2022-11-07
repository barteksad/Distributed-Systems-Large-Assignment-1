use std::sync::Arc;
use std::{sync::atomic::AtomicI16};
use std::time::{Duration};
use assignment_1_solution::{Handler, ModuleRef, System};
use async_channel::{Sender, unbounded};
use log::debug;
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
    _timer1.stop().await;
    _timer1.stop().await;
    _timer1.stop().await;
    _timer2.stop().await;
    _timer2.stop().await;
    _timer2.stop().await;
    _timer2.stop().await;
    _timer2.stop().await;
    _timer3.stop().await;
    _timer3.stop().await;
    _timer3.stop().await;
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

#[derive(Clone)]
struct Pass;

struct Circle {
    other: Option<ModuleRef<Circle>>,
    n_passes: i32,
    passes: i32,
    sleep: Duration,
    output : Option<Sender<()>>
}

impl Circle {
    fn new(o : Option<Sender<()>>) -> Self {
        Circle {
            other:None,
            n_passes:10,
            passes:0,
            sleep: Duration::from_millis(100),
            output: o,
        }
    }
}

#[derive(Clone)]
struct Init {
    target: ModuleRef<Circle>,
}


#[async_trait::async_trait]
impl Handler<Pass> for Circle {
    async fn handle(&mut self, _self_ref: &ModuleRef<Self> , _msg:Pass) {
        debug!("target: {:?}, passes: {}", self.other.is_some(), self.passes);
        self.passes = self.passes + 1;
        if self.passes >= self.n_passes {
            if let Some(s) = &self.output {
                _ = s.send(()).await;
            } else {
                self.other.as_ref().unwrap().send(Pass).await;
            }
        } else {
            self.other.as_ref().unwrap().send(Pass).await;
        }

        tokio::time::sleep(self.sleep).await;
    }
}

#[async_trait::async_trait]
impl Handler<Init> for Circle {
    async fn handle(&mut self, _self_ref: &ModuleRef<Self> , msg:Init) {
        self.other = Some(msg.target);
    }
}

#[tokio::test]
#[timeout(2500)]
async fn time_circle() {
    let mut sys = System::new().await;
    let mut circles = Vec::new();
    let (tx, rx) = unbounded::<()>();
    let n = 1000;

    for _ in 0..n-1 {
        circles.push(sys.register_module(Circle::new(None)).await);
    }

    circles.push(sys.register_module(Circle::new(Some(tx))).await);

    for i in 0..n-1 {
        circles.get(i).unwrap().send(Init{
            target : (*circles.get(i+1).as_ref().unwrap()).clone()
        }).await;
    }

    circles.get(n-1).unwrap().send(Init{
        target : (*circles.get(0).as_ref().unwrap()).clone()
    }).await;

    (*circles.get(0).as_ref().unwrap()).send(Pass).await;

    _ = rx.recv().await;
    sys.shutdown().await;

}