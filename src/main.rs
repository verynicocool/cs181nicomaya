use assets_manager::{asset::Png, AssetCache};
use frenderer::{
    input::{Input, Key},
    sprites::{Camera2D, SheetRegion, Transform},
    wgpu, Renderer,
};
use rand::Rng;
mod geom;
mod grid;
use geom::*;

#[derive(Debug, PartialEq, Eq)]
enum EntityType {
    Player,
    Enemy,
    // which level, grid x in dest level, grid y in dest level
    #[allow(dead_code)]
    Door(String, u16, u16),
}

#[derive(Clone, Copy, Debug)]
struct TileData {
    solid: bool,
    sheet_region: SheetRegion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Dir {
    N,
    E,
    S,
    W,
}

const PLAYER: [SheetRegion; 4] = [
    //n, e, s, w
    SheetRegion::rect(461 + 16 * 2, 39, 16, 16),
    SheetRegion::rect(461, 39, 16, 16),
    SheetRegion::rect(461 + 16 * 3, 39, 16, 16),
    SheetRegion::rect(461 + 16, 39, 16, 16),
];
const PLAYER_ATK: [SheetRegion; 4] = [
    //n, e, s, w
    SheetRegion::rect(428, 0, 16, 8), // offset by 8px in direction
    SheetRegion::rect(349, 22, 8, 16),
    SheetRegion::rect(162, 13, 16, 8),
    SheetRegion::rect(549, 17, 8, 16),
];
const ENEMY: [SheetRegion; 4] = [
    SheetRegion::rect(533 + 16 * 2, 39, 16, 16),
    SheetRegion::rect(533 + 16, 39, 16, 16),
    SheetRegion::rect(533, 39, 16, 16),
    SheetRegion::rect(533 + 16 * 3, 39, 16, 16),
];

const HEART: SheetRegion = SheetRegion::rect(525, 35, 8, 8);

impl Dir {
    fn to_vec2(self) -> Vec2 {
        match self {
            Dir::N => Vec2 { x: 0.0, y: 1.0 },
            Dir::E => Vec2 { x: 1.0, y: 0.0 },
            Dir::S => Vec2 { x: 0.0, y: -1.0 },
            Dir::W => Vec2 { x: -1.0, y: 0.0 },
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pos {
    pos: Vec2,
    dir: Dir,
    alive: bool,
}
mod level;
use level::Level;
struct Game {
    current_level: usize,
    levels: Vec<Level>,
    enemies: Vec<Pos>,
    player: Pos,
    attack_area: Rect,
    attack_timer: f32,
    knockback_timer: f32,
    health: u8,
}

// Feel free to change this if you use a different tilesheet
const TILE_SZ: usize = 16;
const W: usize = 320;
const H: usize = 240;

// pixels per second
const PLAYER_SPEED: f32 = 64.0;
const ENEMY_SPEED: f32 = 32.0;
const KNOCKBACK_SPEED: f32 = 128.0;

const ATTACK_MAX_TIME: f32 = 0.3;
const ATTACK_COOLDOWN_TIME: f32 = 0.1;
const KNOCKBACK_TIME: f32 = 0.25;

const DT: f32 = 1.0 / 60.0;

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    let source =
        assets_manager::source::FileSystem::new("content").expect("Couldn't load resources");
    #[cfg(target_arch = "wasm32")]
    let source = assets_manager::source::Embedded::from(assets_manager::source::embed!("content"));
    let cache = assets_manager::AssetCache::with_source(source);

    let drv = frenderer::Driver::new(
        winit::window::WindowBuilder::new()
            .with_title("test")
            .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 768.0)),
        Some((W as u32, H as u32)),
    );

    let mut input = Input::default();

    let mut now = frenderer::clock::Instant::now();
    let mut acc = 0.0;
    drv.run_event_loop::<(), _>(
        move |window, mut frend| {
            let game = Game::new(&mut frend, &cache);
            (window, game, frend)
        },
        move |event, target, (window, ref mut game, ref mut frend)| {
            use winit::event::{Event, WindowEvent};
            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    target.exit();
                }
                Event::WindowEvent {
                    event: WindowEvent::Resized(size),
                    ..
                } => {
                    if !frend.gpu.is_web() {
                        frend.resize_surface(size.width, size.height);
                    }
                    window.request_redraw();
                }
                Event::WindowEvent {
                    event: WindowEvent::RedrawRequested,
                    ..
                } => {
                    let elapsed = now.elapsed().as_secs_f32();
                    // You can add the time snapping/death spiral prevention stuff here if you want.
                    // I'm not using it here to keep the starter code small.
                    acc += elapsed;
                    now = std::time::Instant::now();
                    // While we have time to spend
                    while acc >= DT {
                        // simulate a frame
                        acc -= DT;
                        game.simulate(&input, DT);
                        input.next_frame();
                    }
                    game.render(frend);
                    frend.render();
                    window.request_redraw();
                }
                event => {
                    input.process_input_event(&event);
                }
            }
        },
    )
    .expect("event loop error");
}

impl Game {
    fn new(renderer: &mut Renderer, cache: &AssetCache) -> Self {
        let tile_handle = cache
            .load::<Png>("texture")
            .expect("Couldn't load tilesheet img");
        let tile_img = tile_handle.read().0.to_rgba8();
        let tile_tex = renderer.create_array_texture(
            &[&tile_img],
            wgpu::TextureFormat::Rgba8UnormSrgb,
            tile_img.dimensions(),
            Some("tiles-sprites"),
        );
        let levels = vec![
            Level::from_str(
                &cache
                    .load::<String>("level1")
                    .expect("Couldn't access level1.txt")
                    .read(),
            ),
            Level::from_str(
                &cache
                    .load::<String>("level2")
                    .expect("Couldn't access level2.txt")
                    .read(),
            ),
        ];
        let current_level = 0;
        let camera = Camera2D {
            screen_pos: [0.0, 0.0],
            screen_size: [W as f32, H as f32],
        };
        let sprite_estimate =
            levels[current_level].sprite_count() + levels[current_level].starts().len();
        renderer.sprite_group_add(
            &tile_tex,
            vec![Transform::ZERO; sprite_estimate],
            vec![SheetRegion::ZERO; sprite_estimate],
            camera,
        );
        let player_start = *levels[current_level]
            .starts()
            .iter()
            .find(|(t, _)| *t == EntityType::Player)
            .map(|(_, ploc)| ploc)
            .expect("Start level doesn't put the player anywhere");
        let mut game = Game {
            current_level,
            attack_area: Rect {
                x: 0.0,
                y: 0.0,
                w: 0,
                h: 0,
            },
            knockback_timer: 0.0,
            attack_timer: 0.0,
            levels,
            health: 3,
            enemies: vec![],
            player: Pos {
                pos: player_start,
                dir: Dir::S,
                alive: true,
            },
        };
        game.enter_level(player_start);
        game
    }
    fn level(&self) -> &Level {
        &self.levels[self.current_level]
    }
    fn enter_level(&mut self, player_pos: Vec2) {
        self.enemies.clear();
        self.player.pos = player_pos;
        for (etype, pos) in self.levels[self.current_level].starts().iter() {
            match etype {
                EntityType::Player => {}
                EntityType::Door(_rm, _x, _y) => {}
                EntityType::Enemy => self.enemies.push(Pos {
                    pos: *pos,
                    dir: Dir::S,
                    alive: true,
                }),
            }
        }
    }
    fn sprite_count(&self) -> usize {
        let base_count = self.level().sprite_count() + self.enemies.len() + 2;
        let heart_count = self.health as usize;
        base_count + heart_count
    }

    fn check_collision_with_walls(&self, rect: Rect) -> Vec2 {
        let tiles = self.level().tiles_within(rect);
        let mut displacement = Vec2 { x: 0.0, y: 0.0 };

        for (tile_rect, tile_data) in tiles {
            if tile_data.solid {
                if let Some(overlap) = rect.overlap(tile_rect) {
                    if overlap.x < overlap.y {
                        displacement.x += overlap.x * (rect.x - tile_rect.x).signum();
                    } else {
                        displacement.y += overlap.y * (rect.y - tile_rect.y).signum();
                    }
                }
            }
        }
        displacement
    }
    // test
    fn render(&mut self, frend: &mut Renderer) {
        // make this exactly as big as we need
        frend.sprite_group_resize(0, self.sprite_count());

        let sprites_used = self.level().render_into(frend, 0);
        let (sprite_posns, sprite_gfx) = frend.sprites_mut(0, sprites_used..);

        for (enemy, (trf, uv)) in self
            .enemies
            .iter()
            .filter(|enemy| enemy.alive)
            .zip(sprite_posns.iter_mut().zip(sprite_gfx.iter_mut()))
        {
            *trf = Transform {
                w: TILE_SZ as u16,
                h: TILE_SZ as u16,
                x: enemy.pos.x,
                y: enemy.pos.y,
                rot: 0.0,
            };
            *uv = ENEMY[enemy.dir as usize];
        }

        let sprite_posns = &mut sprite_posns[self.enemies.len()..];
        let sprite_gfx = &mut sprite_gfx[self.enemies.len()..];
        sprite_posns[0] = Transform {
            w: TILE_SZ as u16,
            h: TILE_SZ as u16,
            x: self.player.pos.x,
            y: self.player.pos.y,
            rot: if self.health > 0 { 0.0 } else { 90.0 },
        };
        sprite_gfx[0] = PLAYER[self.player.dir as usize].with_depth(1);
        if self.attack_area.is_empty() {
            sprite_posns[1] = Transform::ZERO;
        } else {
            let (w, h) = match self.player.dir {
                Dir::N | Dir::S => (16, 8),
                _ => (8, 16),
            };
            let delta = self.player.dir.to_vec2() * 7.0;
            sprite_posns[1] = Transform {
                w,
                h,
                x: self.player.pos.x + delta.x,
                y: self.player.pos.y + delta.y,
                rot: 0.0,
            };
        }
        sprite_gfx[1] = PLAYER_ATK[self.player.dir as usize].with_depth(0);

        let heart_start_x = 8.0 + self.player.pos.x
            - ((self.health as f32 * 8.0 + (self.health as f32 - 1.0) * 4.0) / 2.0);
        let heart_y = self.player.pos.y + 15.0;

        let start_index_for_hearts = 2;

        for i in 0..self.health {
            let heart_x = heart_start_x + (i as f32) * (8.0 + 1.0);
            sprite_posns[start_index_for_hearts + i as usize] = Transform {
                x: heart_x,
                y: heart_y,
                w: 8,
                h: 8,
                rot: 0.0,
            };
            sprite_gfx[start_index_for_hearts + i as usize] = HEART;
        }
    }
    fn simulate(&mut self, input: &Input, dt: f32) {
        let mut dx = 0.0;
        let mut dy = 0.0;

        if self.attack_timer > 0.0 {
            self.attack_timer -= dt;
        }
        if self.knockback_timer > 0.0 {
            self.knockback_timer -= dt;
        }
        if self.health > 0 {
            dx = input.key_axis(Key::ArrowLeft, Key::ArrowRight) * PLAYER_SPEED * DT;
            dy = input.key_axis(Key::ArrowDown, Key::ArrowUp) * PLAYER_SPEED * DT;

            let attacking = !self.attack_area.is_empty();
            let knockback = self.knockback_timer > 0.0;
            if attacking {
                // while attacking we can't move
                dx = 0.0;
                dy = 0.0;
            } else if knockback {
                // during knockback we move but don't turn around
                let delta = self.player.dir.to_vec2();
                dx = -delta.x * KNOCKBACK_SPEED * dt;
                dy = -delta.y * KNOCKBACK_SPEED * dt;
            } else {
                // not attacking, no knockback, do normal movement
                if dx > 0.0 {
                    self.player.dir = Dir::E;
                }
                if dx < 0.0 {
                    self.player.dir = Dir::W;
                }
                if dy > 0.0 {
                    self.player.dir = Dir::N;
                }
                if dy < 0.0 {
                    self.player.dir = Dir::S;
                }
            }
        }
        if self.health > 0 {
            if self.attack_timer <= 0.0 && input.is_key_pressed(Key::Space) {
                // TODO POINT: compute the attack area's center based on the player's position and facing and some offset
                // For the spritesheet provided, the attack is placed 8px "forwards" from the player.
                self.attack_timer = ATTACK_MAX_TIME;
                let attack_direction = self.player.dir.to_vec2();
                let attack_offset = 10.0;
                self.attack_area = Rect {
                    x: self.player.pos.x + attack_direction.x * attack_offset
                        - (TILE_SZ as f32 / 2.0),
                    y: self.player.pos.y + attack_direction.y * attack_offset
                        - (TILE_SZ as f32 / 2.0),
                    w: match self.player.dir {
                        Dir::N | Dir::S => TILE_SZ as u16,
                        _ => 10,
                    },
                    h: match self.player.dir {
                        Dir::E | Dir::W => TILE_SZ as u16,
                        _ => 10,
                    },
                };
            } else if self.attack_timer <= ATTACK_COOLDOWN_TIME {
                self.attack_area = Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 0,
                    h: 0,
                };
            }
        }

        let dest = self.player.pos + Vec2 { x: dx, y: dy };
        self.player.pos = dest;
        let mut rng = rand::thread_rng();
        for enemy in self.enemies.iter_mut() {
            if rng.gen_bool(0.05) {
                enemy.dir = match rng.gen_range(0..4) {
                    0 => Dir::N,
                    1 => Dir::E,
                    2 => Dir::S,
                    3 => Dir::W,
                    _ => panic!(),
                };
            }
            enemy.pos += enemy.dir.to_vec2() * ENEMY_SPEED * dt;
        }

        let player_rect = Rect {
            x: self.player.pos.x - (TILE_SZ as f32 / 2.0),
            y: self.player.pos.y - (TILE_SZ as f32 / 2.0),
            w: TILE_SZ as u16,
            h: TILE_SZ as u16,
        };
        let player_displacement = self.check_collision_with_walls(player_rect);
        self.player.pos += player_displacement;

        // Step 2: Calculate displacements for all enemies
        let enemy_displacements: Vec<Vec2> = self
            .enemies
            .iter()
            .map(|enemy| {
                let enemy_rect = Rect {
                    x: enemy.pos.x - (TILE_SZ as f32 / 2.0),
                    y: enemy.pos.y - (TILE_SZ as f32 / 2.0),
                    w: TILE_SZ as u16,
                    h: TILE_SZ as u16,
                };
                self.check_collision_with_walls(enemy_rect)
            })
            .collect();

        // Step 3: Apply displacements to enemies
        for (enemy, displacement) in self.enemies.iter_mut().zip(enemy_displacements.iter()) {
            enemy.pos += *displacement;
        }

        for enemy in self.enemies.iter_mut() {
            let enemy_rect = Rect {
                x: enemy.pos.x - (TILE_SZ as f32 / 2.0),
                y: enemy.pos.y - (TILE_SZ as f32 / 2.0),
                w: TILE_SZ as u16,
                h: TILE_SZ as u16,
            };

            if self.attack_area.overlap(enemy_rect).is_some() && enemy.alive {
                enemy.alive = false;
            }
        }
        if self.health > 0 && self.knockback_timer <= 0.0 {
            let player_rect = Rect {
                x: self.player.pos.x - (TILE_SZ as f32 / 2.0),
                y: self.player.pos.y - (TILE_SZ as f32 / 2.0),
                w: TILE_SZ as u16,
                h: TILE_SZ as u16,
            };

            for enemy in &self.enemies {
                let enemy_rect = Rect {
                    x: enemy.pos.x - (TILE_SZ as f32 / 2.0),
                    y: enemy.pos.y - (TILE_SZ as f32 / 2.0),
                    w: TILE_SZ as u16,
                    h: TILE_SZ as u16,
                };

                if player_rect.overlap(enemy_rect).is_some() && enemy.alive {
                    self.health = self.health.saturating_sub(1);
                    self.knockback_timer = KNOCKBACK_TIME;

                    let knockback_direction = Vec2 {
                        x: self.player.pos.x - enemy.pos.x,
                        y: self.player.pos.y - enemy.pos.y,
                    }
                    .normalize();

                    let knockback_strength = 10.0;
                    self.player.pos += knockback_direction * knockback_strength;

                    break;
                }
            }
        }

        // ----
        // TODO POINT: implement collision detection here. for collision with the tilemap, you can use Level::tiles_within to find the tiles touching a rectangle, and filter out the ones that are not solid.  Then you have rects you can test against your player/enemies.
        // TODO POINT: damage and knock back the player (you can use knockback_timer & health fields of game; you want the player to be invulnerable temporarily after hitting an enemy, so just decreasing health on its own won't work!)
        // TODO POINT: damage/destroy the enemy

        // I suggest gathering player-tile collisions and enemy-tile collisions, then doing collision response on those sets of contacts (it's OK to use two sets of contacts).
        // You could have helper functions like gather_contacts_tiles(&[rect], &Level, &mut Vec<Contact>) and gather_contacts(&[rect], &[rect], &mut Vec<Contact>) or do_collision_response(&[Contact], &mut [rect], &[rect]) or compute_displacement(rect, rect) -> Vec2.
        // A Contact struct is not strictly necessary but it's a good idea (with fields like displacement, a_index, a_rect, b_index, and b_rect fields).
        // Then, you can check for contacts between the player & their attack rectangle on one side, and the enemies on the other side (you can reuse gather_contacts for this).  These don't need to participate in collision response, but you can use them to determine whether the player or enemy should be damaged.

        // For deleting enemies, it's best to add the enemy to a "to_remove" vec, and then remove those enemies after this loop is all done.
        // Alternatively, you could "disable" an enemy by giving it an `alive` flag or similar and setting that to false, not drawing or updating dead enemies.
    }
}
