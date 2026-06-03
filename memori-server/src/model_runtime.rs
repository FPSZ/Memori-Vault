use crate::*;

mod settings;

pub(crate) use settings::*;

#[cfg(test)]
mod tests;

pub(crate) async fn replace_engine(
    engine_slot: &Arc<Mutex<Option<MemoriEngine>>>,
    init_error: &Arc<Mutex<Option<String>>>,
    watch_root: PathBuf,
    reason: &str,
) -> Result<(), String> {
    let previous_engine = {
        let mut guard = engine_slot.lock().await;
        guard.take()
    };

    if let Some(engine) = previous_engine {
        match timeout(
            Duration::from_secs(ENGINE_SHUTDOWN_TIMEOUT_SECS),
            engine.shutdown(),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                warn!(error = %err, "old engine shutdown failed; rebuilding anyway");
            }
            Err(_) => {
                warn!(
                    timeout_secs = ENGINE_SHUTDOWN_TIMEOUT_SECS,
                    "old engine shutdown timed out; rebuilding anyway"
                );
            }
        }
    }

    let result: Result<(), String> = async {
        let settings = load_app_settings()?;
        let policy = resolve_enterprise_policy(&settings);
        let model_settings = resolve_model_settings(&settings);
        let active_runtime = resolve_active_runtime_settings(&model_settings);
        validate_runtime_model_settings(
            &to_model_policy(&policy),
            &to_runtime_model_config(&active_runtime),
        )
        .map_err(|violation| violation.message)?;
        apply_model_settings_to_env(active_runtime);

        let mut new_engine =
            MemoriEngine::bootstrap(watch_root.clone()).map_err(|err| err.to_string())?;
        new_engine
            .set_indexing_config(resolve_indexing_config(&settings))
            .await;
        new_engine.start_daemon().map_err(|err| err.to_string())?;

        {
            let mut guard = engine_slot.lock().await;
            *guard = Some(new_engine);
        }
        {
            let mut init_guard = init_error.lock().await;
            *init_guard = None;
        }

        info!(
            reason = reason,
            watch_root = %watch_root.display(),
            "memori-server daemon started"
        );

        Ok(())
    }
    .await;

    if let Err(err) = &result {
        let mut init_guard = init_error.lock().await;
        *init_guard = Some(err.clone());
    }

    result
}
