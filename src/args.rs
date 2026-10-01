use std::{env, path::PathBuf};

pub const HELP: &str = "\
roose - a desktop goose for Wayland, written in Rust

USAGE: roose [OPTIONS]

OPTIONS:
  --size <px>        goose width in logical pixels [default: 80]
  --speed <factor>   movement speed multiplier [default: 1.0]
  --fps <n>          ticks per second while moving [default: 30]
  --cooldown <s>     minimum seconds between fetched windows [default: 45]
  --sprite <png>     use your own goose image (should face right)
  --memes <dir>      folder with images to drag in [default: ~/Pictures/roose]
  --editor <cmd>     command for notes [default: \"gedit --new-window\"]
  --viewer <cmd>     command for memes [default: imv]
  --no-steal         never steal the cursor
  --no-notes         never drag in notes
  --no-memes         never drag in memes
  -h, --help         show this help
";

pub struct Args {
    pub size: f32,
    pub speed: f32,
    pub fps: f32,
    pub cooldown: f32,
    pub sprite: Option<PathBuf>,
    pub memes: Option<PathBuf>,
    pub editor: Vec<String>,
    pub viewer: Vec<String>,
    pub steal: bool,
    pub notes: bool,
    pub meme_props: bool,
}

impl Args {
    pub fn parse() -> Result<Self, String> {
        let mut args = Self {
            size: 80.0,
            speed: 1.0,
            fps: 30.0,
            cooldown: 45.0,
            sprite: None,
            memes: env::var_os("HOME").map(|h| PathBuf::from(h).join("Pictures/roose")),
            editor: words("gedit --new-window"),
            viewer: words("imv"),
            steal: true,
            notes: true,
            meme_props: true,
        };
        let mut raw = env::args().skip(1);
        while let Some(flag) = raw.next() {
            let mut value = || raw.next().ok_or_else(|| format!("{flag} needs a value"));
            match flag.as_str() {
                "--size" => args.size = number(&value()?)?,
                "--speed" => args.speed = number(&value()?)?,
                "--fps" => args.fps = number(&value()?)?,
                "--cooldown" => args.cooldown = number(&value()?)?,
                "--sprite" => args.sprite = Some(value()?.into()),
                "--memes" => args.memes = Some(value()?.into()),
                "--editor" => args.editor = words(&value()?),
                "--viewer" => args.viewer = words(&value()?),
                "--no-steal" => args.steal = false,
                "--no-notes" => args.notes = false,
                "--no-memes" => args.meme_props = false,
                "-h" | "--help" => {
                    print!("{HELP}");
                    std::process::exit(0);
                }
                other => return Err(format!("unknown option {other}\n\n{HELP}")),
            }
        }
        if args.size < 8.0 || args.fps < 1.0 || args.speed <= 0.0 {
            return Err("--size must be >= 8, --fps >= 1 and --speed > 0".into());
        }
        Ok(args)
    }
}

fn words(command: &str) -> Vec<String> {
    command.split_whitespace().map(String::from).collect()
}

fn number(value: &str) -> Result<f32, String> {
    value
        .parse()
        .map_err(|_| format!("{value} is not a number"))
}
