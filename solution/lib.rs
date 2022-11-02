use async_channel::{unbounded, Receiver, Sender};
use log::debug;
use tokio::{task::JoinHandle, time::Instant};
use std::{time::Duration, collections::LinkedList};

pub trait Message: Send + 'static {}
impl<T: Send + 'static> Message for T {}

pub trait Module: Send + 'static {}
impl<T: Send + 'static> Module for T {}

/// A trait for modules capable of handling messages of type `M`.
#[async_trait::async_trait]
pub trait Handler<M: Message>: Module {
    /// Handles the message. A module must be able to access a `ModuleRef` to itself through `self_ref`.
    async fn handle(&mut self, self_ref: &ModuleRef<Self>, msg: M);
}

#[async_trait::async_trait]
trait Handlee<T: Module>: Message {
    async fn get_handled(self: Box<Self>, module_ref: &ModuleRef<T>, t: &mut T);
}

#[async_trait::async_trait]
impl<M: Message, T: Handler<M>> Handlee<T> for M {
    async fn get_handled(self: Box<Self>, module_ref: &ModuleRef<T>, t: &mut T) {
        t.handle(module_ref, *self).await
    }
}

/// A handle returned by `ModuleRef::request_tick()`, can be used to stop sending further ticks.
// You can add fields to this struct
pub struct TimerHandle {
    tx: Sender<()>,
}

impl TimerHandle {
    /// Stops the sending of ticks resulting from the corresponding call to `ModuleRef::request_tick()`.
    /// If the ticks are already stopped, does nothing.
    pub async fn stop(&self) {
        _ = self.tx.send(()).await;
    }
}

async fn module_loop<T: Module>(mut module: T, module_ref: ModuleRef<T>, msg_rx: Receiver<Box<dyn Handlee<T>>>, shutdown_rx: Receiver<()>) {
    loop {
        debug!("In loop");
        // Because tokio::select is random first check if we are after shutdown
        if let Ok(_) = shutdown_rx.try_recv() {
            debug!("Received signal to shutdown!");
            break;
        }

        tokio::select! {
            Ok(_) = shutdown_rx.recv() => {
                debug!("Received signal to shutdown!");
                break;
            },
            Ok(msg) = msg_rx.recv() => {
                debug!("module received new message");
                msg.get_handled(&module_ref, &mut module).await;
                debug!("Message handeled!");
            }
            // Err(e) = shutdown_rx.recv() => {
            //     debug!("{:?}", e);
            //     break;
            // },
            // Err(e) = msg_rx.recv() => {
            //     debug!("{:?}", e);
            //     break;
            // }
        }
    }

    debug!("After loop");
}

// You can add fields to this struct.
pub struct System {
    join_handles: LinkedList<JoinHandle<()>>,
    shutdown_txrx : (Sender<()>, Receiver<()>),
}

impl System {
    /// Registers the module in the system.
    /// Returns a `ModuleRef`, which can be used then to send messages to the module.
    pub async fn register_module<T: Module>(&mut self, module: T) -> ModuleRef<T> {
        debug!("Registering new module");
        let (msg_tx, msg_rx) = unbounded::<Box<dyn Handlee<T>>>();
        let module_ref = ModuleRef { msg_tx };
        let shutdown_rx = self.shutdown_txrx.1.clone();

        let join_handle = tokio::spawn(module_loop(module, module_ref.clone(), msg_rx, shutdown_rx));
        self.join_handles.push_back(join_handle);

        module_ref
    }

    /// Creates and starts a new instance of the system.
    pub async fn new() -> Self {
        System {
            join_handles: LinkedList::new(),
            shutdown_txrx: unbounded(),
        }
    }

    /// Gracefully shuts the system down.
    pub async fn shutdown(&mut self) {
        debug!("Shutting down module...");
        for _ in 0..self.join_handles.len() {
            _ = self.shutdown_txrx.0.send(()).await;
        }

        for task in self.join_handles.iter_mut() {
            if let Err(e) = task.await {
                debug!("Error in shutdown {:?}", e);
            }
        }
        debug!("Done!");
    }
}

/// A reference to a module used for sending messages.
// You can add fields to this struct.
pub struct ModuleRef<T: Module + ?Sized> {
    msg_tx: Sender<Box<dyn Handlee<T>>>,
}

impl<T: Module> ModuleRef<T> {
    /// Sends the message to the module.
    pub async fn send<M: Message>(&self, msg: M)
    where
        T: Handler<M>,
    {
        debug!("Sending new message...");
        if let Err(e) = self.msg_tx.send(Box::new(msg)).await {
            debug!("{:?}", e);
        }
        debug!("Done!");
    }

    /// Schedules a message to be sent to the module periodically with the given interval.
    /// The first tick is sent after the interval elapses.
    /// Every call to this function results in sending new ticks and does not cancel
    /// ticks resulting from previous calls.
    pub async fn request_tick<M>(&self, message: M, delay: Duration) -> TimerHandle
    where
        M: Message + Clone,
        T: Handler<M>,
    {
        let (tx, rx) = unbounded::<()>();
        let msg_tx = self.msg_tx.clone();
        let mut interval = tokio::time::interval_at(Instant::now() + delay, delay);

        tokio::spawn( async move {
            loop {
                tokio::select! {
                    Ok(_) = rx.recv() => {
                        break;
                    },
                    _ = interval.tick() => {
                        let msg = Box::new(message.clone());
                        if let Err(_) = msg_tx.send(msg).await {
                            break;
                        }
                    }
                }
            }
        });

        TimerHandle { tx }

    }
}

impl<T: Module> Clone for ModuleRef<T> {
    /// Creates a new reference to the same module.
    fn clone(&self) -> Self {
        debug!("Module cloned!");
        ModuleRef { msg_tx: self.msg_tx.clone() }
    }
}