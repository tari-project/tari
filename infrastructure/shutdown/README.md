# A convenient shutdown signal

`ShutdownSignal` is a convenient wrapper around a one-shot channel that allows different threads to let each other know
that they should stop working.

## Basic usage

First, create the shutdown signal.

    let mut shutdown = Shutdown::new();

Use `to_signal` to create a future which will resolve when `Shutdown` is triggered.

    let signal = shutdown.to_signal();
    assert_eq!(shutdown.is_triggered(), false);

You can clone the signal and move it into threads that need to be informed of when to shut down. We're using tokio here,
but this will work in any futures-based runtime:

    tokio::spawn(async move { 
        signal.await.unwrap(); 
        println!("Finished");    
    });

Then when you want to trigger the shutdown signal, call `trigger`. All signals will resolve.

    shutdown.trigger().unwrap();   // "Finished" is printed
    // Shutdown::trigger is idempotent
    shutdown.trigger().unwrap();
    assert_eq!(shutdown.is_triggered(), true);

_Note_: If the ShutdownSignal instance is dropped, it will trigger the signal, so the `Shutdown` instance should be held
as long as required by the application.
