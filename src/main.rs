use std::env;
use std::rc::Rc;
use tokio::time;

use deno_core::{anyhow::Result, resolve_path, v8, FsModuleLoader, JsRuntime, RuntimeOptions};

#[tokio::main]
async fn main() -> Result<()> {
    let cwd = env::current_dir()?;
    let module_path = env::args().nth(1).unwrap_or_else(|| "hello.js".to_string());
    let module_spec = resolve_path(&module_path, &cwd)?;
    println!("Running {module_path}");

    let memory_limit = 10 * 1024 * 1024;
    let timeout = time::Duration::from_secs(2);

    let mut runtime = JsRuntime::new(RuntimeOptions {
        module_loader: Some(Rc::new(FsModuleLoader)),
        create_params: Some(v8::CreateParams::default().heap_limits(0, memory_limit)),
        ..Default::default()
    });

    // Terminate isolate when approaching memory limit
    let cb_handle = runtime.v8_isolate().thread_safe_handle();
    runtime.add_near_heap_limit_callback(move |current_limit, _initial_limit| {
        println!("Terminating isolate near memory limit (current={current_limit} max={memory_limit})");
        cb_handle.terminate_execution();
        current_limit * 2
    });

    // Start controller thread to terminate isolate after timeout
    let cb_handle = runtime.v8_isolate().thread_safe_handle();
    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(timeout) => {
                println!("Terminating isolate after timeout ({timeout:?})");
                cb_handle.terminate_execution();
            }
        }
    });

    let module_id = runtime.load_main_module(&module_spec, None).await?;

    let mut receiver = runtime.mod_evaluate(module_id);
    tokio::select! {
      biased;

      maybe_result = &mut receiver => {
        println!("Result = {maybe_result:#?}");
        maybe_result.expect("Module evaluation result not provided.")
      }

      event_loop_result = runtime.run_event_loop(false) => {
        event_loop_result?;
        let maybe_result = receiver.await;
        maybe_result.expect("Module evaluation result not provided.")
      }
    }
}
