use super::*;

impl AsyncHookQueue {
    pub fn spawn(
        plugin_manager: Arc<PluginManager>,
        maintenance_cancel_rx: tokio::sync::watch::Receiver<bool>,
        shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel::<AsyncHookJob>(ASYNC_HOOK_QUEUE_CAPACITY);
        let shared_rx = Arc::new(tokio::sync::Mutex::new(rx));

        for worker_idx in 0..ASYNC_HOOK_WORKERS {
            let pm = Arc::clone(&plugin_manager);
            let rx_ref = Arc::clone(&shared_rx);
            let mut maint_rx = maintenance_cancel_rx.clone();
            let mut shut_rx = shutdown_rx.clone();
            tokio::spawn(async move {
                loop {
                    if *shut_rx.borrow() {
                        break;
                    }
                    if *maint_rx.borrow() {
                        {
                            let mut guard = rx_ref.lock().await;
                            while guard.try_recv().is_ok() {}
                        }
                        tokio::select! {
                            biased;
                            _ = wait_watch_bool(&mut shut_rx, true) => break,
                            _ = wait_watch_bool(&mut maint_rx, false) => {},
                        }
                        continue;
                    }

                    let maybe_job = {
                        let mut guard = rx_ref.lock().await;
                        tokio::select! {
                            biased;
                            _ = wait_watch_bool(&mut shut_rx, true) => None,
                            _ = wait_watch_bool(&mut maint_rx, true) => None,
                            recv = guard.recv() => recv,
                        }
                    };

                    let Some(job) = maybe_job else {
                        if *shut_rx.borrow() {
                            break;
                        }
                        if *maint_rx.borrow() {
                            continue;
                        }
                        break;
                    };

                    let fut = run_async_hook_job(&pm, job);
                    tokio::select! {
                        biased;
                        _ = wait_watch_bool(&mut shut_rx, true) => {
                            let mut guard = rx_ref.lock().await;
                            while guard.try_recv().is_ok() {}
                            break;
                        }
                        _ = wait_watch_bool(&mut maint_rx, true) => {
                            let mut guard = rx_ref.lock().await;
                            while guard.try_recv().is_ok() {}
                        }
                        _ = fut => {}
                    }
                }
                let _ = worker_idx;
            });
        }

        Self {
            tx,
            shared_rx,
            maint_rx: maintenance_cancel_rx,
            shut_rx: shutdown_rx,
        }
    }

    pub fn enqueue(&self, job: AsyncHookJob) -> bool {
        if job.dispatch_depth >= 2 || *self.maint_rx.borrow() || *self.shut_rx.borrow() {
            return false;
        }
        match self.tx.try_send(job) {
            Ok(()) => {
                if *self.maint_rx.borrow() || *self.shut_rx.borrow() {
                    if let Ok(handle) = tokio::runtime::Handle::try_current() {
                        let rx_ref = Arc::clone(&self.shared_rx);
                        handle.spawn(async move {
                            let mut guard = rx_ref.lock().await;
                            while guard.try_recv().is_ok() {}
                        });
                    }
                    return false;
                }
                true
            }
            Err(err) => {
                let event_name = match &err {
                    tokio::sync::mpsc::error::TrySendError::Full(j)
                    | tokio::sync::mpsc::error::TrySendError::Closed(j) => j.event.as_str(),
                };
                eprintln!("AsyncHookQueue drop {event_name}: queue full or closed");
                false
            }
        }
    }
}
