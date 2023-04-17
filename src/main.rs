use std::env;
use std::rc::Rc;

use deno_core::{anyhow::Result, resolve_path, FsModuleLoader, JsRuntime, RuntimeOptions};
use log::debug;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or(log::Level::Info.to_level_filter().to_string()),
    )
    .try_init()?;

    let mut runtime = JsRuntime::new(RuntimeOptions {
        module_loader: Some(Rc::new(FsModuleLoader)),
        ..Default::default()
    });

    let cwd = env::current_dir()?;
    let module_spec = resolve_path("code.js", &cwd)?;
    let module_id = runtime.load_main_module(&module_spec, None).await?;

    let mut receiver = runtime.mod_evaluate(module_id);
    tokio::select! {
      // Not using biased mode leads to non-determinism for relatively simple
      // programs.
      biased;

      maybe_result = &mut receiver => {
        debug!("received module evaluate {:#?}", maybe_result);
        maybe_result.expect("Module evaluation result not provided.")
      }

      event_loop_result = runtime.run_event_loop(false) => {
        event_loop_result?;
        let maybe_result = receiver.await;
        maybe_result.expect("Module evaluation result not provided.")
      }
    }
}
