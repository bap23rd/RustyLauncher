//! What the Servers page does: show installed servers, edit their launch settings, open their folders,
//! rename and delete them. Installing and updating go through the download queue (see `downloads`).

use crate::ctx::{self, Ctx};
use crate::{ConfigField, ServerConfig, ServerTile, actions, format, tasks};
use rl_core::model::{Server, ServerInstallType, State};
use rl_server::{FlagOption, ServerLaunchConfig, TextOption, UMOD_URL};
use std::path::PathBuf;

// ---------------------------------------------------------------- the library

pub fn tiles(state: &State) -> Vec<ServerTile> {
    let mut tiles: Vec<ServerTile> = state.servers.iter().map(tile).collect();
    tiles.sort_by_key(|t| t.name.to_lowercase());
    tiles
}

fn tile(s: &Server) -> ServerTile {
    let present = s.install_path.join(rl_server::SERVER_EXE).is_file();
    ServerTile {
        id: s.id.as_str().into(),
        name: s.name.as_str().into(),
        kind: match s.install_type {
            ServerInstallType::Latest => "latest",
            ServerInstallType::Pinned => "pinned",
        }
        .into(),
        manifest: s.manifest_id.clone().unwrap_or_else(|| s.id.clone()).into(),
        created: s
            .created_at
            .as_deref()
            .map(format::iso_date)
            .unwrap_or_default()
            .into(),
        oxide: present && rl_server::has_oxide(&s.install_path),
        missing: !present,
    }
}

fn path_of(ctx: &Ctx, id: &str) -> Option<(PathBuf, String)> {
    ctx.state
        .borrow()
        .server_by_id(id)
        .map(|s| (s.install_path.clone(), s.name.clone()))
}

// ---------------------------------------------------------------- settings dialog

fn field(o: &TextOption) -> ConfigField {
    ConfigField {
        enabled: o.enabled,
        value: o.value.as_str().into(),
    }
}

fn option(f: &ConfigField) -> TextOption {
    TextOption {
        enabled: f.enabled,
        value: f.value.trim().to_string(),
    }
}

fn to_ui(c: &ServerLaunchConfig) -> ServerConfig {
    ServerConfig {
        hostname: field(&c.hostname),
        identity: field(&c.identity),
        port: field(&c.port),
        rcon_port: field(&c.rcon_port),
        rcon_password: field(&c.rcon_password),
        rcon_web: c.rcon_web.enabled,
        max_players: field(&c.max_players),
        world_size: field(&c.world_size),
        seed: field(&c.seed),
        save_interval: field(&c.save_interval),
        custom_args: c.custom_args.as_str().into(),
    }
}

fn from_ui(u: &ServerConfig) -> ServerLaunchConfig {
    ServerLaunchConfig {
        hostname: option(&u.hostname),
        identity: option(&u.identity),
        port: option(&u.port),
        rcon_port: option(&u.rcon_port),
        rcon_password: option(&u.rcon_password),
        rcon_web: FlagOption {
            enabled: u.rcon_web,
        },
        max_players: option(&u.max_players),
        world_size: option(&u.world_size),
        seed: option(&u.seed),
        save_interval: option(&u.save_interval),
        custom_args: u.custom_args.trim().to_string(),
    }
}

pub fn configure(id: &str) {
    let ctx = ctx::get();
    let Some((dir, name)) = path_of(&ctx, id) else {
        ctx.toast("That server is no longer in the library.", "error");
        return;
    };
    let config = match rl_server::read_server_file(&dir) {
        Ok(file) => file.map(|f| f.config).unwrap_or_default(),
        Err(err) => {
            tracing::warn!(%err, "damaged server settings file");
            ctx.toast(
                "This server's settings file is damaged, so defaults are shown. Saving replaces it.",
                "error",
            );
            ServerLaunchConfig::default()
        }
    };
    ctx.with_app(|a| {
        a.set_server_cfg(to_ui(&config));
        a.set_config_title(format!("Launch settings: {name}").into());
        a.set_config_error("".into());
        a.set_config_server_id(id.into());
        a.set_config_open(true);
    });
}

pub fn save_config() {
    let ctx = ctx::get();
    let Some(app) = ctx.app.upgrade() else { return };
    let id = app.get_config_server_id().to_string();
    let config = from_ui(&app.get_server_cfg());
    if let Some(problem) = config.problems().first() {
        app.set_config_error(problem.message.as_str().into());
        return;
    }
    let Some((dir, _)) = path_of(&ctx, &id) else {
        app.set_config_open(false);
        return;
    };
    match rl_server::save_config(&dir, config) {
        Ok(()) => {
            app.set_config_open(false);
            ctx.toast("Settings saved. RustServer.bat was updated too.", "success");
        }
        Err(err) => {
            tracing::error!(%err, "could not save the server settings");
            app.set_config_error(format!("Could not save: {err}").as_str().into());
        }
    }
}

pub fn new_password() {
    let ctx = ctx::get();
    ctx.with_app(|a| {
        let mut cfg = a.get_server_cfg();
        cfg.rcon_password.value = rl_server::random_password(16).into();
        cfg.rcon_password.enabled = true;
        a.set_server_cfg(cfg);
    });
}

// ---------------------------------------------------------------- folders

pub fn browse(id: &str) {
    let ctx = ctx::get();
    match path_of(&ctx, id) {
        Some((dir, _)) if dir.is_dir() => open(&ctx, &dir),
        _ => ctx.toast("That folder could not be found.", "error"),
    }
}

/// Open the Oxide folder, or point the user at uMod if Oxide is not installed.
pub fn mods(id: &str) {
    let ctx = ctx::get();
    let Some((dir, _)) = path_of(&ctx, id) else {
        ctx.toast("That server is no longer in the library.", "error");
        return;
    };
    if rl_server::has_oxide(&dir) {
        open(&ctx, &rl_server::mods_dir(&dir));
    } else {
        ctx.toast(
            "Oxide is not installed on this server. Opening uMod, where you can get it.",
            "info",
        );
        actions::open_url(UMOD_URL);
    }
}

fn open(ctx: &Ctx, path: &std::path::Path) {
    if let Err(err) = std::process::Command::new("explorer").arg(path).spawn() {
        tracing::warn!(%err, "could not open Explorer");
        ctx.toast("Could not open the folder.", "error");
    }
}

// ---------------------------------------------------------------- rename and delete

pub fn rename(ctx: &Ctx, id: &str, new_name: &str) {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        ctx.toast("Please enter a name.", "error");
        return;
    }
    let Some((dir, _)) = path_of(ctx, id) else {
        ctx.toast("That server is no longer in the library.", "error");
        return;
    };
    match rl_server::rename(&dir, new_name) {
        Ok(()) => {
            if let Some(s) = ctx
                .state
                .borrow_mut()
                .servers
                .iter_mut()
                .find(|s| s.id == id)
            {
                s.name = new_name.to_string();
            }
            ctx.save();
            ctx.refresh();
            ctx.toast("Renamed.", "success");
        }
        Err(err) => ctx.toast(&format!("Could not rename: {err}"), "error"),
    }
}

pub fn delete(ctx: &Ctx, id: &str) {
    let Some((dir, name)) = path_of(ctx, id) else {
        ctx.toast("That server is no longer in the library.", "error");
        return;
    };
    let id = id.to_string();
    tasks::run(
        ctx,
        format!("Deleting {name}"),
        false,
        move |rep, _cancel| {
            rep.note(&format!("Deleting {name}..."));
            if !dir.exists() {
                return Ok(());
            }
            // Same safety rule as for game versions: only a folder that looks like a server install.
            if !rl_server::looks_like_server(&dir) {
                return Err(format!(
                    "Refusing to delete {}: it does not look like a Rust server.",
                    dir.display()
                ));
            }
            std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
        },
        move |ctx, result| match result {
            Ok(()) => {
                ctx.state.borrow_mut().remove_server(&id);
                ctx.save();
                ctx.refresh();
                ctx.toast("Server deleted.", "success");
            }
            Err(e) => ctx.toast(&e, "error"),
        },
    );
}
