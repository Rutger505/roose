use crate::geometry::Vec2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prop {
    Note,
    Meme,
}

impl Prop {
    pub fn size(self) -> Vec2 {
        match self {
            Self::Note => Vec2::new(560.0, 260.0),
            Self::Meme => Vec2::new(480.0, 360.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub id: String,
    pub size: Vec2,
}

/// Everything the goose can do to the desktop. Coordinates are local to the goose's monitor.
pub trait Desktop {
    fn cursor(&mut self) -> Option<Vec2>;
    fn warp_cursor(&mut self, to: Vec2);
    fn spawn(&mut self, prop: Prop) -> bool;
    /// Returns the spawned window once it has appeared, floated, resized and placed at `top_left`.
    fn take_spawned_window(&mut self, prop: Prop, top_left: Vec2) -> Option<Window>;
    fn move_window(&mut self, window: &Window, top_left: Vec2);
}

pub struct Settings {
    pub size: Vec2,
    pub speed: f32,
    pub frame: f32,
    pub steal_cursor: bool,
    pub props: Vec<Prop>,
    pub prop_cooldown: f32,
}

#[derive(Debug)]
enum State {
    Idle {
        until: f32,
    },
    Wander {
        target: Vec2,
    },
    Chase {
        give_up_at: f32,
    },
    Steal {
        flee_to: Vec2,
        until: f32,
    },
    ToEdge {
        prop: Prop,
        facing: Facing,
        target: Vec2,
    },
    AwaitProp {
        prop: Prop,
        facing: Facing,
        give_up_at: f32,
    },
    Drag {
        window: Window,
        facing: Facing,
        target: Vec2,
    },
}

const WALK: f32 = 90.0;
const RUN: f32 = 280.0;
const DRAG: f32 = 70.0;
const GRIP: f32 = 24.0;

pub struct Goose {
    settings: Settings,
    screen: Vec2,
    pos: Vec2,
    facing: Facing,
    state: State,
    clock: f32,
    walked: f32,
    next_prop_at: f32,
    rng: fastrand::Rng,
}

impl Goose {
    pub fn new(settings: Settings, screen: Vec2, rng: fastrand::Rng) -> Self {
        let mut goose = Self {
            screen,
            pos: Vec2::default(),
            facing: Facing::Right,
            state: State::Idle { until: 1.0 },
            clock: 0.0,
            walked: 0.0,
            next_prop_at: settings.prop_cooldown / 2.0,
            settings,
            rng,
        };
        goose.pos = goose.random_point();
        goose
    }

    pub fn facing(&self) -> Facing {
        self.facing
    }

    /// Top-left of the sprite including the waddle bob.
    pub fn draw_position(&self) -> Vec2 {
        let bob = if self.is_walking() {
            (self.walked * 0.3).sin().abs() * 4.0
        } else {
            0.0
        };
        Vec2::new(self.pos.x, self.pos.y - bob)
    }

    fn is_walking(&self) -> bool {
        !matches!(self.state, State::Idle { .. } | State::AwaitProp { .. })
    }

    fn beak_offset(&self, facing: Facing) -> Vec2 {
        let (bx, by) = crate::sprite::BEAK;
        let x = match facing {
            Facing::Right => bx,
            Facing::Left => 1.0 - bx,
        };
        Vec2::new(x * self.settings.size.x, by * self.settings.size.y)
    }

    fn beak(&self) -> Vec2 {
        self.pos + self.beak_offset(self.facing)
    }

    fn walk_room(&self) -> Vec2 {
        self.screen - self.settings.size
    }

    fn random_point(&mut self) -> Vec2 {
        let room = self.walk_room();
        Vec2::new(
            self.rng.f32() * room.x.max(0.0),
            self.rng.f32() * room.y.max(0.0),
        )
    }

    fn clamp_to_screen(&self, point: Vec2) -> Vec2 {
        let room = self.walk_room();
        Vec2::new(
            point.x.clamp(0.0, room.x.max(0.0)),
            point.y.clamp(0.0, room.y.max(0.0)),
        )
    }

    /// Walks towards `target`; `face` overrides the facing, otherwise the goose looks where it goes.
    fn walk(&mut self, target: Vec2, speed: f32, dt: f32, face: Option<Facing>) -> bool {
        let (next, arrived) = self
            .pos
            .step_towards(target, speed * self.settings.speed * dt);
        let dx = next.x - self.pos.x;
        self.walked += self.pos.distance(next);
        self.pos = next;
        self.facing = match face {
            Some(facing) => facing,
            None if dx > 0.01 => Facing::Right,
            None if dx < -0.01 => Facing::Left,
            None => self.facing,
        };
        arrived
    }

    fn idle(&mut self, min: f32, max: f32) -> State {
        State::Idle {
            until: self.clock + min + self.rng.f32() * (max - min),
        }
    }

    fn pick_mischief(&mut self, desktop: &mut dyn Desktop) -> State {
        let roll = self.rng.f32();
        if self.settings.steal_cursor && roll < 0.2 && desktop.cursor().is_some() {
            return State::Chase {
                give_up_at: self.clock + 7.0,
            };
        }
        if roll < 0.4 && self.clock >= self.next_prop_at && !self.settings.props.is_empty() {
            let prop = self.settings.props[self.rng.usize(..self.settings.props.len())];
            let facing = if self.rng.bool() {
                Facing::Left
            } else {
                Facing::Right
            };
            let size = prop.size();
            let room = self.walk_room();
            let beak_y = self.beak_offset(facing).y;
            let min_y = (size.y / 2.0 - beak_y).max(0.0);
            let max_y = (self.screen.y - size.y / 2.0 - beak_y).clamp(min_y, room.y.max(min_y));
            let target = Vec2::new(
                if facing == Facing::Left {
                    0.0
                } else {
                    room.x.max(0.0)
                },
                min_y + self.rng.f32() * (max_y - min_y),
            );
            return State::ToEdge {
                prop,
                facing,
                target,
            };
        }
        State::Wander {
            target: self.random_point(),
        }
    }

    /// Where a dragged window goes so the goose appears to hold it in its beak.
    fn window_origin(&self, window_size: Vec2, facing: Facing) -> Vec2 {
        let beak = self.beak();
        let x = match facing {
            Facing::Left => beak.x - window_size.x + GRIP,
            Facing::Right => beak.x - GRIP,
        };
        Vec2::new(x, beak.y - window_size.y / 2.0)
    }

    /// Starts the stolen window fully beyond the monitor edge; subsequent drag ticks pull it in.
    fn window_spawn_origin(&self, window_size: Vec2, facing: Facing) -> Vec2 {
        let attached = self.window_origin(window_size, facing);
        let x = match facing {
            Facing::Left => -window_size.x,
            Facing::Right => self.screen.x,
        };
        Vec2::new(x, attached.y)
    }

    /// Advances the goose by `dt` seconds and returns how long it may sleep before the next tick.
    pub fn tick(&mut self, dt: f32, desktop: &mut dyn Desktop) -> f32 {
        self.clock += dt;
        // Keep real time for timers, but never teleport after the system slept.
        let dt = dt.min(0.1);
        let frame = self.settings.frame;
        let state = std::mem::replace(&mut self.state, State::Idle { until: 0.0 });
        self.state = match state {
            State::Idle { until } if self.clock < until => State::Idle { until },
            State::Idle { .. } => self.pick_mischief(desktop),

            State::Wander { target } => {
                if self.walk(target, WALK, dt, None) {
                    self.idle(1.0, 5.0)
                } else {
                    State::Wander { target }
                }
            }

            State::Chase { give_up_at } => match desktop.cursor() {
                Some(cursor) if self.clock < give_up_at => {
                    let direction = if cursor.x >= self.pos.x {
                        Facing::Right
                    } else {
                        Facing::Left
                    };
                    let target = self.clamp_to_screen(cursor - self.beak_offset(direction));
                    self.walk(target, RUN, dt, Some(direction));
                    if self.beak().distance(cursor) < 12.0 {
                        let flee_to = self.far_point();
                        State::Steal {
                            flee_to,
                            until: self.clock + 3.5,
                        }
                    } else {
                        State::Chase { give_up_at }
                    }
                }
                _ => self.idle(1.0, 3.0),
            },

            State::Steal { flee_to, until } => {
                let arrived = self.walk(flee_to, RUN * 0.8, dt, None);
                desktop.warp_cursor(self.beak());
                if arrived || self.clock >= until {
                    self.idle(2.0, 5.0)
                } else {
                    State::Steal { flee_to, until }
                }
            }

            State::ToEdge {
                prop,
                facing,
                target,
            } => {
                if self.walk(target, WALK, dt, None) {
                    self.facing = facing;
                    if desktop.spawn(prop) {
                        State::AwaitProp {
                            prop,
                            facing,
                            give_up_at: self.clock + 20.0,
                        }
                    } else {
                        self.idle(1.0, 3.0)
                    }
                } else {
                    State::ToEdge {
                        prop,
                        facing,
                        target,
                    }
                }
            }

            State::AwaitProp {
                prop,
                facing,
                give_up_at,
            } => {
                let origin = self.window_spawn_origin(prop.size(), facing);
                match desktop.take_spawned_window(prop, origin) {
                    Some(window) => {
                        self.next_prop_at = self.clock + self.settings.prop_cooldown;
                        let target = self.drag_target(&window, facing);
                        State::Drag {
                            window,
                            facing,
                            target,
                        }
                    }
                    None if self.clock >= give_up_at => self.idle(1.0, 3.0),
                    None => {
                        self.state = State::AwaitProp {
                            prop,
                            facing,
                            give_up_at,
                        };
                        return 0.3;
                    }
                }
            }

            State::Drag {
                window,
                facing,
                target,
            } => {
                let arrived = self.walk(target, DRAG, dt, Some(facing));
                desktop.move_window(&window, self.window_origin(window.size, facing));
                if arrived {
                    self.idle(2.0, 6.0)
                } else {
                    State::Drag {
                        window,
                        facing,
                        target,
                    }
                }
            }
        };

        match self.state {
            State::Idle { until } => (until - self.clock).max(frame),
            _ => frame,
        }
    }

    fn far_point(&mut self) -> Vec2 {
        let min = self.screen.x.min(self.screen.y) * 0.4;
        let mut best = self.random_point();
        for _ in 0..8 {
            if best.distance(self.pos) >= min {
                break;
            }
            best = self.random_point();
        }
        best
    }

    /// Picks where the goose stops so the dragged window ends up fully on screen.
    fn drag_target(&mut self, window: &Window, facing: Facing) -> Vec2 {
        let beak = self.beak_offset(facing).x;
        let spare = (self.screen.x - window.size.x).max(0.0);
        let window_x = spare * (0.1 + self.rng.f32() * 0.4);
        let x = match facing {
            Facing::Left => window_x + window.size.x - GRIP - beak,
            Facing::Right => self.screen.x - window_x - window.size.x + GRIP - beak,
        };
        self.clamp_to_screen(Vec2::new(x, self.pos.y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeDesktop {
        cursor: Option<Vec2>,
        warps: Vec<Vec2>,
        spawned: Vec<Prop>,
        window_ready: bool,
        moves: Vec<Vec2>,
    }

    impl Desktop for FakeDesktop {
        fn cursor(&mut self) -> Option<Vec2> {
            self.cursor
        }
        fn warp_cursor(&mut self, to: Vec2) {
            self.cursor = Some(to);
            self.warps.push(to);
        }
        fn spawn(&mut self, prop: Prop) -> bool {
            self.spawned.push(prop);
            true
        }
        fn take_spawned_window(&mut self, prop: Prop, _: Vec2) -> Option<Window> {
            self.window_ready.then(|| Window {
                id: "0xgoose".into(),
                size: prop.size(),
            })
        }
        fn move_window(&mut self, _: &Window, top_left: Vec2) {
            self.moves.push(top_left);
        }
    }

    fn goose(steal_cursor: bool, props: Vec<Prop>) -> Goose {
        let settings = Settings {
            size: Vec2::new(80.0, 80.0),
            speed: 1.0,
            frame: 1.0 / 30.0,
            steal_cursor,
            props,
            prop_cooldown: 1000.0,
        };
        Goose::new(
            settings,
            Vec2::new(1920.0, 1080.0),
            fastrand::Rng::with_seed(7),
        )
    }

    fn run(goose: &mut Goose, desktop: &mut FakeDesktop, seconds: f32) {
        let mut t = 0.0;
        while t < seconds {
            let dt = goose.tick(1.0 / 30.0, desktop).min(1.0 / 30.0);
            t += dt;
        }
    }

    #[test]
    fn stays_on_screen() {
        let mut goose = goose(false, vec![]);
        let mut desktop = FakeDesktop::default();
        for _ in 0..30_000 {
            goose.tick(1.0 / 30.0, &mut desktop);
            assert!(goose.pos.x >= 0.0 && goose.pos.x <= 1840.0);
            assert!(goose.pos.y >= 0.0 && goose.pos.y <= 1000.0);
        }
    }

    #[test]
    fn idles_without_burning_frames() {
        let mut goose = goose(false, vec![]);
        goose.state = State::Idle { until: 4.0 };
        let sleep = goose.tick(0.0, &mut FakeDesktop::default());
        assert!(sleep > 3.9);
    }

    #[test]
    fn long_sleeps_count_as_real_time() {
        let mut goose = goose(false, vec![]);
        goose.state = State::Idle { until: 3.0 };
        goose.tick(3.5, &mut FakeDesktop::default());
        assert!(!matches!(goose.state, State::Idle { .. }));
    }

    #[test]
    fn steals_the_cursor() {
        let mut goose = goose(true, vec![]);
        let mut desktop = FakeDesktop {
            cursor: Some(Vec2::new(900.0, 500.0)),
            ..Default::default()
        };
        goose.state = State::Chase { give_up_at: 100.0 };
        run(&mut goose, &mut desktop, 10.0);
        assert!(!desktop.warps.is_empty(), "goose never grabbed the cursor");
        assert!(
            desktop
                .warps
                .last()
                .unwrap()
                .distance(Vec2::new(900.0, 500.0))
                > 50.0
        );
    }

    #[test]
    fn drags_a_prop_on_screen() {
        let mut goose = goose(false, vec![Prop::Note]);
        let mut desktop = FakeDesktop {
            window_ready: true,
            ..Default::default()
        };
        goose.state = State::Idle { until: 0.0 };
        goose.rng = fastrand::Rng::with_seed(1);
        goose.next_prop_at = 0.0;
        for _ in 0..60 {
            run(&mut goose, &mut desktop, 60.0);
            if !desktop.moves.is_empty() && matches!(goose.state, State::Idle { .. }) {
                break;
            }
        }
        assert_eq!(desktop.spawned, vec![Prop::Note]);
        let last = *desktop.moves.last().expect("window was never dragged");
        let size = Prop::Note.size();
        assert!(
            last.x >= 0.0 && last.x + size.x <= 1920.0,
            "window ended at {last:?}"
        );
        assert!(
            last.y >= 0.0 && last.y + size.y <= 1080.0,
            "window ended at {last:?}"
        );
    }
}
