use super::runtime;
use crate::{platform::desktop, settings};

pub(crate) fn run() {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments == [std::ffi::OsString::from("--portal-service")] {
        env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("warn,tern=info"),
        )
        .init();
        if let Err(error) = crate::browser::chooser::run() {
            eprintln!("tern portal: {error:#}");
            std::process::exit(1);
        }
        return;
    }
    let service_mode = arguments == [std::ffi::OsString::from("--file-manager-service")];
    if arguments == [std::ffi::OsString::from("--help")] {
        println!("Usage: tern [--select] [--] [PATH|file://URI ...]");
        return;
    }
    if arguments == [std::ffi::OsString::from("--version")] {
        println!("tern {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let targets = match desktop::parse_arguments(if service_mode { Vec::new() } else { arguments })
    {
        Ok(targets) => targets,
        Err(error) => {
            eprintln!("tern: {error:#}");
            std::process::exit(2);
        }
    };
    let initial = (!service_mode).then_some(desktop::OpenRequest { targets });
    let config = settings::config_dir_path();
    let default_config = home::home_dir().unwrap_or_default().join(".config/tern");
    let name = desktop::instance_name(&config, &default_config);
    let is_default_profile = name == "io.github.coolloong.Tern";
    let instance = match desktop::connect_instance(&name, initial.as_ref(), is_default_profile) {
        Ok(Some(instance)) => instance,
        Ok(None) => return,
        Err(error) => {
            eprintln!("tern: {error:#}");
            std::process::exit(1);
        }
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,tern=info"))
        .init();

    log::info!(
        "tern {} starting; HOME={:?} XDG_CONFIG_HOME={:?}",
        env!("CARGO_PKG_VERSION"),
        std::env::var("HOME"),
        std::env::var("XDG_CONFIG_HOME")
    );
    log::info!("config dir: {:?}", settings::config_dir_path());
    log::info!("settings path: {:?}", settings::settings_json_path());

    let settings = match settings::load_settings_from_disk() {
        Ok(s) => s,
        Err(err) => {
            log::warn!("failed to load settings.json, using defaults: {err:#}");
            settings::SettingsFile::default()
        }
    };
    log::info!(
        "loaded settings: theme={:?} show_hidden={}",
        settings.appearance.theme,
        settings.appearance.show_hidden_files
    );

    runtime::run(settings, initial, instance);
}
