use std::{
    collections::HashSet,
    env, fs,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use crate::{
    brain::{Desktop, Prop, Window},
    geometry::Vec2,
    hypr::Hyprland,
};

const QUOTES: &[&str] = &[
    "HONK. I have taken your cursor and I will do it again.",
    "Your code compiles. For now. -- the goose",
    "Peace was never an option.",
    "I am not a duck. Stop calling me a duck.",
    "Rewrote myself in Rust. Now I am blazingly annoying.",
    "Did you mean: git push --force? I did it for you. HONK.",
    "You have 3 unsaved files. Correction: you had.",
    "The borrow checker let me borrow your mouse. I am not giving it back.",
    "Todo: 1. Honk  2. Steal bread  3. Honk again",
    "This window was brought to you by a goose with no respect for your workflow.",
    "Zero-cost abstractions. Infinite-cost goose.",
    "I live in your compositor now.",
];

pub struct Commands {
    pub editor: Vec<String>,
    pub viewer: Vec<String>,
    pub memes: Option<PathBuf>,
}

struct Pending {
    needle: String,
    known: HashSet<String>,
}

pub struct HyprDesktop {
    hypr: Option<Hyprland>,
    origin: Vec2,
    commands: Commands,
    cache: PathBuf,
    pending: Option<Pending>,
    children: Vec<Child>,
    spawned: u32,
    rng: fastrand::Rng,
}

impl HyprDesktop {
    pub fn new(
        hypr: Option<Hyprland>,
        origin: Vec2,
        commands: Commands,
        rng: fastrand::Rng,
    ) -> Self {
        let cache = env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
            .unwrap_or_else(env::temp_dir)
            .join("roose");
        Self {
            hypr,
            origin,
            commands,
            cache,
            pending: None,
            children: Vec::new(),
            spawned: 0,
            rng,
        }
    }

    fn write_note(&mut self, name: &str) -> Option<PathBuf> {
        let path = self.cache.join(name);
        let quote = QUOTES[self.rng.usize(..QUOTES.len())];
        fs::write(&path, format!("{quote}\n")).ok()?;
        Some(path)
    }

    fn pick_meme(&mut self) -> Option<PathBuf> {
        let memes: Vec<PathBuf> = self
            .commands
            .memes
            .as_deref()
            .and_then(|dir| fs::read_dir(dir).ok())
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| is_image(path))
            .collect();
        if memes.is_empty() {
            let fallback = self.cache.join("goose.png");
            if !fallback.exists() {
                crate::sprite::Source::Builtin
                    .render(400, 400)
                    .save_png(&fallback)
                    .ok()?;
            }
            return Some(fallback);
        }
        Some(memes[self.rng.usize(..memes.len())].clone())
    }

    fn launch(&mut self, command: &[String], file: &Path) -> bool {
        self.children
            .retain_mut(|child| matches!(child.try_wait(), Ok(None)));
        let Some((program, args)) = command.split_first() else {
            return false;
        };
        let child = Command::new(program)
            .args(args)
            .arg(file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            // Own process group so Ctrl+C on the goose doesn't close the stolen windows.
            .process_group(0)
            .spawn();
        match child {
            Ok(child) => {
                self.children.push(child);
                true
            }
            Err(err) => {
                eprintln!("roose: could not run {program}: {err}");
                false
            }
        }
    }

    fn global(&self, local: Vec2) -> (i32, i32) {
        (
            (local.x + self.origin.x).round() as i32,
            (local.y + self.origin.y).round() as i32,
        )
    }
}

impl Desktop for HyprDesktop {
    fn cursor(&mut self) -> Option<Vec2> {
        Some(self.hypr.as_ref()?.cursor()? - self.origin)
    }

    fn warp_cursor(&mut self, to: Vec2) {
        let (x, y) = self.global(to);
        if let Some(hypr) = &self.hypr {
            hypr.dispatch(&[format!("movecursor {x} {y}")]);
        }
    }

    fn spawn(&mut self, prop: Prop) -> bool {
        let Some(hypr) = &self.hypr else { return false };
        let known = hypr.clients().into_iter().map(|c| c.address).collect();
        if fs::create_dir_all(&self.cache).is_err() {
            return false;
        }
        self.spawned += 1;
        let (file, command) = match prop {
            Prop::Note => {
                let name = format!("honk-{}-{}.txt", std::process::id(), self.spawned);
                (self.write_note(&name), self.commands.editor.clone())
            }
            Prop::Meme => (self.pick_meme(), self.commands.viewer.clone()),
        };
        let Some(file) = file else { return false };
        let needle = file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let launched = self.launch(&command, &file);
        self.pending = launched.then_some(Pending { needle, known });
        launched
    }

    fn take_spawned_window(&mut self, prop: Prop, top_left: Vec2) -> Option<Window> {
        let hypr = self.hypr.as_ref()?;
        let pending = self.pending.as_ref()?;
        let client = hypr
            .clients()
            .into_iter()
            .find(|c| !pending.known.contains(&c.address) && c.title.contains(&pending.needle))?;
        self.pending = None;

        let size = prop.size();
        let (x, y) = self.global(top_left);
        let target = format!("address:{}", client.address);
        hypr.dispatch(&[
            format!("setfloating {target}"),
            format!(
                "resizewindowpixel exact {} {},{target}",
                size.x as i32, size.y as i32
            ),
            format!("movewindowpixel exact {x} {y},{target}"),
        ]);
        Some(Window {
            id: client.address,
            size,
        })
    }

    fn move_window(&mut self, window: &Window, top_left: Vec2) {
        let (x, y) = self.global(top_left);
        if let Some(hypr) = &self.hypr {
            hypr.dispatch(&[format!(
                "movewindowpixel exact {x} {y},address:{}",
                window.id
            )]);
        }
    }
}

fn is_image(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg"
    )
}
