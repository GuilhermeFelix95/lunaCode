pub mod accounts;
pub mod adblock;
pub mod translate;
pub mod api_profiles;
pub mod auth;
pub mod background_tasks;
pub mod browser;
pub mod browser_actions;
pub mod browser_bookmarks;
pub mod browser_tabs;
#[cfg(test)]
mod browser_tests;
pub mod browser_history;
pub mod browser_import;
pub mod changes;
pub mod claude_plugins;
pub mod cli_update;
pub mod worktrees;
pub mod checkpoints;
pub mod logfile;
pub mod commands;
pub mod conversations;
pub mod favicon;
pub mod files;
pub mod marketplace;
pub mod migrate;
pub mod orchestrator;
pub mod mcp;
pub mod models;
pub mod permission_prompt;
pub mod playwright;
pub mod plugins;
pub mod project;
pub mod providers;
pub mod rules;
pub mod reload;
pub mod site_clone;
pub mod site_preview;
pub mod session;
pub mod setup;
pub mod skills;
pub mod tasks_mcp;
pub mod slash;
pub mod spec;
pub mod terminal;

/// Bytes do PNG do ícone, embutidos em tempo de compilação a partir do arquivo do repositório.
/// O executável em modo de dev não recebe o ícone embutido pelo instalador, então ele é
/// aplicado às janelas na inicialização.
fn window_icon_bytes() -> &'static [u8] {
    include_bytes!("../icons/icon.png")
}

/// Monta o ícone a partir de bytes PNG. Erro voltado em texto para o log, sem derrubar o app.
fn icon_from_bytes(bytes: &[u8]) -> Result<tauri::image::Image<'_>, String> {
    tauri::image::Image::from_bytes(bytes).map_err(|e| e.to_string())
}

/// Aplica o ícone do repositório em todas as janelas existentes. Falha vira erro no log.
fn apply_window_icon(app: &tauri::App) -> Result<(), String> {
    use tauri::Manager;
    let windows = app.webview_windows();
    if windows.is_empty() {
        return Err("nenhuma janela aberta para receber o ícone".into());
    }
    let icon = icon_from_bytes(window_icon_bytes())?;
    for (label, window) in windows {
        window.set_icon(icon.clone()).map_err(|e| format!("janela {label}: {e}"))?;
    }
    Ok(())
}

/// Ponto de entrada do app Tauri.
pub fn run() {
    tauri::Builder::default()
        .manage(commands::ActiveRuns::default())
        .manage(commands::SpecMarks::default())
        .manage(background_tasks::BackgroundTasks::default())
        .manage(terminal::Terminals::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            use tauri::Manager;
            // Erros também vão para luna.log na pasta de logs do app.
            if let Ok(dir) = app.path().app_log_dir() {
                logfile::start(&dir);
            }
            // Grava o guia de plugins para a Luna ler. Sem ele o app segue, só sem o guia.
            match app.path().app_data_dir() {
                Ok(dir) => {
                    if let Err(e) = plugins::guide::install(&dir) {
                        logfile::error(&format!("não foi possível gravar o guia de plugins em {}: {e}", dir.display()));
                    }
                }
                Err(e) => logfile::error(&format!("não foi possível localizar a pasta de dados para o guia de plugins: {e}")),
            }
            // Navegador do agente: servidor local e configuração MCP. Sem ele o agente não vê as ferramentas do navegador.
            match app.path().app_data_dir() {
                Ok(dir) => {
                    if let Err(e) = browser::start(app.handle().clone(), &dir) {
                        logfile::error(&format!("{e}"));
                    }
                }
                Err(e) => logfile::error(&format!("não foi possível localizar a pasta de dados para o navegador: {e}")),
            }
            // Pedidos de permissão do Claude: servidor local e configuração MCP, como o do navegador.
            match app.path().app_data_dir() {
                Ok(dir) => {
                    if let Err(e) = permission_prompt::start(app.handle().clone(), &dir) {
                        logfile::error(&e);
                    }
                }
                Err(e) => logfile::error(&format!("não foi possível localizar a pasta de dados para as permissões: {e}")),
            }
            // Navegador do Playwright (alternativa escolhida em Configurações): só grava a configuração MCP.
            match app.path().app_data_dir() {
                Ok(dir) => {
                    orchestrator::start(&dir);
                    if let Err(e) = playwright::start(&dir) {
                        logfile::error(&e);
                    }
                }
                Err(e) => logfile::error(&format!("não foi possível localizar a pasta de dados para o Playwright: {e}")),
            }
            // Plugins padrão (cache de tokens). Falha só vira aviso no terminal: o app abre sem eles.
            match app.path().app_data_dir() {
                Ok(dir) => {
                    if let Err(e) = plugins::seed::install_defaults(&dir) {
                        logfile::error(&format!("{e}"));
                    }
                }
                Err(e) => logfile::error(&format!("não foi possível localizar a pasta de dados para os plugins padrão: {e}")),
            }
            // Ícone da janela: no modo de dev o executável não recebe o ícone embutido,
            // então ele é definido na inicialização com o arquivo do repositório.
            if let Err(e) = apply_window_icon(app) {
                logfile::error(&format!("não foi possível definir o ícone da janela: {e}"));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::send_prompt,
            commands::cancel_prompt,
            permission_prompt::answer_permission,
            background_tasks::start_background_task,
            background_tasks::stop_background_task,
            background_tasks::list_background_tasks,
            claude_plugins::claude_cli_version_ok,
            claude_plugins::claude_plugins_catalog,
            claude_plugins::claude_plugin_details,
            claude_plugins::claude_plugin_install,
            claude_plugins::claude_plugin_uninstall,
            claude_plugins::claude_plugin_set_enabled,
            claude_plugins::claude_plugin_update,
            claude_plugins::claude_marketplaces_list,
            claude_plugins::claude_marketplace_add,
            claude_plugins::claude_marketplace_remove,
            commands::list_plugins,
            commands::read_plugin_entry,
            commands::read_plugin_asset,
            commands::stop_all_for_update,
            commands::check_auth,
            commands::list_models,
            commands::get_changes,
            commands::project_check,
            commands::create_spec,
            commands::list_accounts,
            commands::create_account,
            commands::account_status,
            commands::account_login,
            commands::record_spec_decisions,
            commands::list_project_dir,
            commands::read_project_file,
            commands::write_project_file,
            worktrees::list_git_branches,
            worktrees::current_git_branch,
            worktrees::create_git_worktree,
            reload::config_fingerprint,
            browser::browser_new_tab,
            browser::browser_switch_tab,
            browser::browser_state,
            browser::browser_set_project,
            browser_bookmarks::browser_bookmarks,
            browser_bookmarks::browser_bookmark_add,
            browser_bookmarks::browser_bookmark_remove,
            browser_history::browser_visits,
            browser_import::browser_import_html,
            browser_import::browser_import_passwords,
            browser_import::browser_passwords,
            browser::browser_set_bounds,
            browser::browser_show,
            browser::browser_navigate,
            playwright::playwright_status,
            playwright::playwright_install,
            browser::browser_history,
            browser::browser_reload,
            browser::browser_close_tab,
            browser::browser_set_economy,
            browser::browser_set_adblock,
            browser::browser_translate,
            browser::browser_read,
            browser::browser_set_search_engine,
            site_preview::browser_preview_url,
            site_clone::site_clone_capture,
            favicon::browser_favicon,
            browser::browser_login_suggestions,
            browser::browser_fill_login,
            commands::get_file_diff,
            commands::discard_change,
            commands::checkpoint_take,
            commands::list_files,
            slash::list_slash_items,
            skills::list_skills,
            skills::load_skill_catalog,
            skills::install_skill,
            skills::list_skill_sources,
            skills::add_skill_source,
            skills::remove_skill_source,
            skills::uninstall_skill,
            mcp::list_mcp_servers,
            mcp::set_mcp_enabled,
            mcp::remove_mcp_server,
            mcp::reconfigure_mcp_server,
            mcp::load_mcp_catalog,
            mcp::install_mcp_from_catalog,
            mcp::list_mcp_sources,
            mcp::add_mcp_source,
            mcp::remove_mcp_source,
            mcp::save_mcp_server,
            commands::save_attachment,
            commands::read_attachment,
            commands::save_file_attachment,
            commands::checkpoint_preview,
            commands::checkpoint_restore,
            commands::terminal_open,
            commands::terminal_write,
            commands::terminal_resize,
            commands::terminal_close,
            commands::open_login,
            commands::get_workspace,
            commands::free_workspace,
            orchestrator::get_orchestrator,
            orchestrator::save_orchestrator,
            rules::get_rules,
            rules::save_rules,
            rules::import_rules,
            commands::list_conversations,
            commands::load_conversation,
            commands::save_conversation,
            commands::delete_conversation,
            commands::rename_conversation,
            commands::cli_status,
            cli_update::cli_update_check,
            cli_update::cli_update_run,
            commands::install_cli,
            commands::api_presets,
            commands::list_api_profiles,
            commands::save_api_profile,
            commands::delete_api_profile,
            commands::set_api_profile_key,
            commands::api_profile_has_key,
            commands::test_api_profile,
            commands::refresh_api_models,
            commands::list_provider_models,
            commands::list_catalog_sources,
            commands::add_catalog_source,
            commands::remove_catalog_source,
            commands::load_catalog,
            commands::install_catalog_plugin,
            commands::uninstall_plugin,
            commands::get_plugins_state,
            commands::set_plugin_enabled,
            commands::inspect_local_plugin,
            commands::install_local_plugin,
            commands::create_plugin,
            commands::open_plugins_folder
        ])
        .on_window_event(|window, event| {
            // Fechar o app encerra as tarefas em segundo plano: nenhum processo fica órfão.
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                use tauri::Manager;
                if let Some(tasks) = window.app_handle().try_state::<background_tasks::BackgroundTasks>() {
                    tasks.0.lock().expect("lock das tarefas em segundo plano").stop_all();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o LunaCode");
}

#[cfg(test)]
mod window_icon_tests {
    use super::*;

    #[test]
    fn the_repo_icon_parses_as_a_window_icon() {
        let icon = icon_from_bytes(window_icon_bytes()).expect("o ícone do repositório é um PNG válido");
        assert!(icon.width() > 0);
        assert!(icon.height() > 0);
    }

    /// Bytes que não são PNG não derrubam a inicialização: viram erro tratado.
    #[test]
    fn invalid_icon_bytes_fail_without_panicking() {
        assert!(icon_from_bytes(b"nao-e-png").is_err());
    }
}
