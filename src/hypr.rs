use std::{
    env,
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};

use serde::Deserialize;

use crate::geometry::Vec2;

/// Talks to Hyprland over its control socket directly, so no `hyprctl` process is spawned per frame.
pub struct Hyprland {
    socket: PathBuf,
    dialect: Dialect,
}

/// With a Lua config Hyprland evaluates `dispatch <x>` as `hl.dispatch(<x>)`, so the classic
/// dispatcher strings are rejected and the `hl.dsp.*` API has to be used instead.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Dialect {
    Hyprlang,
    Lua,
}

pub enum Action<'a> {
    Float {
        window: &'a str,
    },
    Resize {
        window: &'a str,
        width: i32,
        height: i32,
    },
    Move {
        window: &'a str,
        x: i32,
        y: i32,
    },
    MoveCursor {
        x: i32,
        y: i32,
    },
}

impl Action<'_> {
    fn render(&self, dialect: Dialect) -> String {
        match (dialect, self) {
            (Dialect::Hyprlang, Self::Float { window }) => format!("setfloating address:{window}"),
            (
                Dialect::Hyprlang,
                Self::Resize {
                    window,
                    width,
                    height,
                },
            ) => {
                format!("resizewindowpixel exact {width} {height},address:{window}")
            }
            (Dialect::Hyprlang, Self::Move { window, x, y }) => {
                format!("movewindowpixel exact {x} {y},address:{window}")
            }
            (Dialect::Hyprlang, Self::MoveCursor { x, y }) => format!("movecursor {x} {y}"),
            (Dialect::Lua, Self::Float { window }) => {
                format!("hl.dsp.window.float({{action=\"set\",window=\"address:{window}\"}})")
            }
            (
                Dialect::Lua,
                Self::Resize {
                    window,
                    width,
                    height,
                },
            ) => {
                format!(
                    "hl.dsp.window.resize({{x={width},y={height},window=\"address:{window}\"}})"
                )
            }
            (Dialect::Lua, Self::Move { window, x, y }) => {
                format!("hl.dsp.window.move({{x={x},y={y},window=\"address:{window}\"}})")
            }
            (Dialect::Lua, Self::MoveCursor { x, y }) => {
                format!("hl.dsp.cursor.move({{x={x},y={y}}})")
            }
        }
    }
}

#[derive(Deserialize)]
pub struct Monitor {
    pub name: String,
    pub focused: bool,
}

#[derive(Deserialize)]
pub struct Client {
    pub address: String,
    pub title: String,
}

#[derive(Deserialize)]
struct CursorPos {
    x: f32,
    y: f32,
}

impl Hyprland {
    pub fn from_env() -> Result<Self, String> {
        let signature = env::var("HYPRLAND_INSTANCE_SIGNATURE")
            .map_err(|_| "roose only runs on Hyprland (HYPRLAND_INSTANCE_SIGNATURE is not set)")?;
        let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
        let socket = PathBuf::from(runtime_dir)
            .join("hypr")
            .join(signature)
            .join(".socket.sock");
        if !socket.exists() {
            return Err(format!(
                "Hyprland socket {} does not exist",
                socket.display()
            ));
        }
        let mut hypr = Self {
            socket,
            dialect: Dialect::Hyprlang,
        };
        hypr.dialect = hypr.detect_dialect();
        Ok(hypr)
    }

    /// Moves the cursor onto itself: harmless, and only the matching dialect answers "ok".
    fn detect_dialect(&self) -> Dialect {
        let Some(cursor) = self.cursor() else {
            return Dialect::Hyprlang;
        };
        let probe = Action::MoveCursor {
            x: cursor.x.round() as i32,
            y: cursor.y.round() as i32,
        };
        let reply = self.request(&format!("dispatch {}", probe.render(Dialect::Hyprlang)));
        if reply.is_ok_and(|r| r.trim() == "ok") {
            Dialect::Hyprlang
        } else {
            Dialect::Lua
        }
    }

    fn request(&self, command: &str) -> io::Result<String> {
        let mut stream = UnixStream::connect(&self.socket)?;
        stream.write_all(command.as_bytes())?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        Ok(response)
    }

    fn request_json<T: for<'de> Deserialize<'de>>(&self, command: &str) -> Option<T> {
        serde_json::from_str(&self.request(command).ok()?).ok()
    }

    pub fn focused_monitor(&self) -> Option<String> {
        self.request_json::<Vec<Monitor>>("j/monitors")?
            .into_iter()
            .find(|m| m.focused)
            .map(|m| m.name)
    }

    pub fn cursor(&self) -> Option<Vec2> {
        self.request_json::<CursorPos>("j/cursorpos")
            .map(|c| Vec2::new(c.x, c.y))
    }

    pub fn clients(&self) -> Vec<Client> {
        self.request_json("j/clients").unwrap_or_default()
    }

    pub fn dispatch(&self, actions: &[Action]) {
        let batch: Vec<String> = actions
            .iter()
            .map(|a| format!("dispatch {}", a.render(self.dialect)))
            .collect();
        let _ = self.request(&format!("[[BATCH]]{}", batch.join(";")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_both_dialects() {
        let action = Action::Move {
            window: "0xabc",
            x: 10,
            y: -5,
        };
        assert_eq!(
            action.render(Dialect::Hyprlang),
            "movewindowpixel exact 10 -5,address:0xabc"
        );
        assert_eq!(
            action.render(Dialect::Lua),
            r#"hl.dsp.window.move({x=10,y=-5,window="address:0xabc"})"#
        );
        assert_eq!(
            Action::Float { window: "0xabc" }.render(Dialect::Lua),
            r#"hl.dsp.window.float({action="set",window="address:0xabc"})"#
        );
    }
}
