use std::env;
use std::rc::Rc;

use deno_core::{anyhow::Result, resolve_path, v8, FsModuleLoader, JsRuntime, RuntimeOptions};

#[tokio::main]
async fn main() -> Result<()> {
    let cwd = env::current_dir()?;
    let module_path = env::args().nth(1).unwrap_or_else(|| "hello.js".to_string());
    let module_spec = resolve_path(&module_path, &cwd)?;
    dbg!(module_path);

    let memory_limit = 100 * 1024 * 1024;

    let mut runtime = JsRuntime::new(RuntimeOptions {
        module_loader: Some(Rc::new(FsModuleLoader)),
        create_params: Some(v8::CreateParams::default().heap_limits(0, memory_limit)),
        ..Default::default()
    });

    // Terminate isolate when approaching memory limit
    let cb_handle = runtime.v8_isolate().thread_safe_handle();
    runtime.add_near_heap_limit_callback(move |current_limit, _initial_limit| {
        dbg!(current_limit);
        cb_handle.terminate_execution();
        current_limit * 2
    });

    let module_id = runtime.load_main_module(&module_spec, None).await?;

    let mut receiver = runtime.mod_evaluate(module_id);
    tokio::select! {
      biased;

      maybe_result = &mut receiver => {
        dbg!(&maybe_result);
        maybe_result.expect("Module evaluation result not provided.")
      }

      event_loop_result = runtime.run_event_loop(false) => {
        event_loop_result?;
        let maybe_result = receiver.await;
        maybe_result.expect("Module evaluation result not provided.")
      }
    }
}
