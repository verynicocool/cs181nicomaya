use assets_manager::{asset::Png, AssetCache};
// check out https://docs.rs/frenderer/latest/frenderer/ and https://github.com/JoeOsborn/frenderer/tree/main/examples for info on frenderer!
use frenderer::{
    input::{Input, Key},
    sprites::{Camera2D, SheetRegion, Transform},
    wgpu, Renderer,
};

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

extern crate rand;
use rand::seq::SliceRandom;
use rand::Rng;
mod geom;
mod grid;
use geom::*;

#[derive(Debug, PartialEq, Eq)]
enum EntityType {
    Player,
    Enemy,
    // which level, x in dest level, y in dest level
    Door(String, u16, u16),
    Gold,
}

#[derive(Clone, Copy)]
struct Entity {
    pos: Vec2,
    dir: Vec2,
    #[allow(dead_code)]
    pattern: MovementPattern,
}

#[derive(Clone, Copy)]
enum MovementPattern {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug)]
struct TileData {
    solid: bool,
    sheet_region: SheetRegion,
}

mod level;
use level::Level;
struct Game {
    level: Level,
    player: Entity,
    enemies: Vec<Entity>,
    frame_counter: u32,
    is_player_alive: bool,
    start_time: std::time::Instant,
    death_time: Option<std::time::Instant>,
    golds: Vec<Vec2>,
    score: u32,
}

// Feel free to change this if you use a different tilesheet
const TILE_SZ: usize = 16;
const W: usize = 320;
const H: usize = 240;

const PLAYER: SheetRegion = SheetRegion::new(0, 16, 630, 0, 18, 16);
const ENEMY: SheetRegion = SheetRegion::new(0, 16, 579, 0, 18, 16);
const GOLD: SheetRegion = SheetRegion::new(0, 699, 193, 0, 13, 11);

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
        Some((1024, 768)),
    );

    const DT: f32 = 1.0 / 50.0;
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

                    if !game.is_player_alive {
                        if let Some(death_time) = game.death_time {
                            if death_time.elapsed().as_secs() >= 3 {
                                println!("You Lose! You collected {} gold", game.score);
                                target.exit();
                            }
                        }
                    } else if game.start_time.elapsed().as_secs() >= 60 {
                        println!("You Win! You collected {} gold", game.score);
                        handle_win(game.score);
                        target.exit();
                    }
                }
                event => {
                    input.process_input_event(&event);
                }
            }
        },
    )
    .expect("event loop error");
}

fn handle_win(score: u32) {
    let initials = prompt_for_initials();
    if let Err(e) = save_score(&initials, score) {
        eprintln!("Error saving score: {}", e);
        return;
    }

    match read_leaderboard() {
        Ok(leaderboard) => display_leaderboard(&leaderboard),
        Err(e) => eprintln!("Error reading leaderboard: {}", e),
    }
}

fn prompt_for_initials() -> String {
    println!("Enter your initials:");
    let mut initials = String::new();
    io::stdin().read_line(&mut initials).expect("Failed to read line");
    initials.trim().to_uppercase()
}

fn display_leaderboard(leaderboard: &[(String, u32)]) {
    println!("Leaderboard");
    println!("----------------");
    println!("Initials\t\tScore");
    for (initials, score) in leaderboard {
        println!("{}\t\t\t{}", initials, score);
    }
}

fn read_leaderboard() -> io::Result<Vec<(String, u32)>> {
    let path = Path::new("leaderboard.txt");
    let file = fs::File::open(path)?;
    let buf_reader = BufReader::new(file);
    let mut leaderboard = vec![];

    for line in buf_reader.lines() {
        let line = line?;
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() == 2 {
            if let Ok(score) = parts[1].parse::<u32>() {
                leaderboard.push((parts[0].to_string(), score));
            }
        }
    }

    // Sort the leaderboard by score in descending order
    leaderboard.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(leaderboard)
}

fn save_score(initials: &str, score: u32) -> io::Result<()> {
    let path = Path::new("leaderboard.txt");
    let mut file = OpenOptions::new()
        .write(true)
        .append(true)
        .create(true)
        .open(path)?;

    writeln!(file, "{},{}", initials, score)
}

impl Entity {
    pub fn new_enemy(pos: Vec2, pattern: MovementPattern) -> Self {
        let initial_dir = match pattern {
            MovementPattern::Horizontal => Vec2 { x: 1.0, y: 0.0 },
            MovementPattern::Vertical => Vec2 { x: 0.0, y: 1.0 },
        };
        Entity {
            pos,
            dir: initial_dir,
            pattern,
        }
    }
}

impl Game {
    fn new(renderer: &mut Renderer, cache: &AssetCache) -> Self {
        let tile_handle = cache
            .load::<Png>("tilesheet")
            .expect("Couldn't load tilesheet img");
        let tile_img = tile_handle.read().0.to_rgba8();
        let tile_tex = renderer.create_array_texture(
            &[&tile_img],
            wgpu::TextureFormat::Rgba8UnormSrgb,
            tile_img.dimensions(),
            Some("tiles-sprites"),
        );
        let level = Level::from_str(
            &cache
                .load::<String>("level1")
                .expect("Couldn't access level1.txt")
                .read(),
        );
        let camera = Camera2D {
            screen_pos: [0.0, 0.0],
            screen_size: [W as f32, H as f32],
        };
        let sprite_estimate = level.sprite_count() + level.starts().len();
        renderer.sprite_group_add(
            &tile_tex,
            vec![Transform::ZERO; sprite_estimate],
            vec![SheetRegion::ZERO; sprite_estimate],
            camera,
        );
        let player_start = *level
            .starts()
            .iter()
            .find(|(t, _)| *t == EntityType::Player)
            .map(|(_, ploc)| ploc)
            .expect("Start level doesn't put the player anywhere");
        let mut game = Game {
            level,
            player: Entity {
                pos: Vec2 { x: 0.0, y: 0.0 },
                dir: Vec2 { x: 0.0, y: 0.0 },
                pattern: MovementPattern::Horizontal,
            },
            enemies: Vec::new(),
            frame_counter: 0,
            is_player_alive: true,
            start_time: std::time::Instant::now(),
            death_time: None,
            golds: Vec::new(),
            score: 0,
        };
        game.enter_level(player_start);
        game.spawn_enemies();
        game.spawn_gold(50);

        game
    }
    fn enter_level(&mut self, player_pos: Vec2) {
        // TODO point: move player to player_pos, delete all enemies and doors,
        // create an entity for each start in level
        self.player.pos = player_pos;

        for (etype, pos) in self.level.starts().iter() {
            match etype {
                EntityType::Player => {}
                EntityType::Door(_rm, _x, _y) => {
                    println!("Would add a door to room: {} at x: {}, y: {}", _rm, _x, _y);
                }
                EntityType::Enemy => {
                    println!("Would spawn an enemy at position: {:?}", pos);
                }
                _ => {
                    // Ignore other types, such as Gold, as they are handled separately or not applicable here.
                }
            }                      
        }
    }


    fn spawn_gold(&mut self, gold_count: i32) {
        let open_spaces = self.level.get_open_spaces();
        let mut rng = rand::thread_rng();

        // Filter open spaces to exclude those occupied by enemies or the player
        let available_spaces: Vec<_> = open_spaces
            .into_iter()
            .filter(|pos| {
                self.player.pos != Vec2 { x: pos.0 as f32, y: pos.1 as f32 }
            })
            .collect();

        // Randomly choose locations from available spaces
        for _ in 0..gold_count {
            if let Some(&position) = available_spaces.choose(&mut rng) {
                // Use `position` directly here
                self.golds.push(Vec2 { x: position.0 as f32, y: position.1 as f32 });
            }
        }

    }

    fn spawn_enemies(&mut self) {
        let open_spaces = self.level.get_open_spaces();
        // remove spaces that are too close to the player
        let open_spaces = open_spaces
            .iter()
            .filter(|&pos| {
                let dx = (self.player.pos.x - pos.0 as f32).abs();
                let dy = (self.player.pos.y - pos.1 as f32).abs();
                dx > 15.0 || dy > 15.0
            })
            .copied()
            .collect::<Vec<_>>();
        let mut rng = rand::thread_rng();
        let enemy_count = rng.gen_range(1..=2);

        for _ in 0..enemy_count {
            if let Some(&position) = open_spaces.choose(&mut rng) {
                let pattern = if rng.gen() {
                    MovementPattern::Horizontal
                } else {
                    MovementPattern::Vertical
                };
                let enemy = Entity::new_enemy(
                    Vec2 {
                        x: position.0 as f32,
                        y: position.1 as f32,
                    },
                    pattern,
                );
                self.enemies.push(enemy);
            }
        }
    }

    fn update_gold(&mut self) {
        let player_size = 0.25;
        let gold_size = 0.25; // Adjust this as necessary

        // Detect golds to remove
        let to_remove: Vec<Vec2> = self.golds.iter().filter_map(|gold_pos| {
            if Self::check_collision(self.player.pos, player_size, *gold_pos, gold_size) {
                Some(*gold_pos)
            } else {
                None
            }
        }).collect();

        // Remove golds that collided
        self.golds.retain(|gold_pos| !to_remove.contains(gold_pos));

        // Spawn a new gold if needed
        if self.golds.len() < 50 {
            self.spawn_gold(1);
            self.score += 1;
        }

    }

    
    fn check_collision(a_pos: Vec2, a_size: f32, b_pos: Vec2, b_size: f32) -> bool {
        let a_half_size = a_size / 2.0;
        let b_half_size = b_size / 2.0;
    
        // Check for overlap in the x-axis
        let x_overlap = (a_pos.x - b_pos.x).abs() < (a_half_size + b_half_size);
        // Check for overlap in the y-axis
        let y_overlap = (a_pos.y - b_pos.y).abs() < (a_half_size + b_half_size);
    
        x_overlap && y_overlap
    }
    
    fn calculate_total_sprites_needed(&self) -> usize {
        let level_tiles = self.level.grid_width() * self.level.grid_height();
        let entity_count = 1 + self.enemies.len() + self.golds.len();

        let other_entities_count = 0;
        level_tiles + entity_count + other_entities_count
    }

    // fn get_enemies_count(&self) -> usize {
    //     self.enemies.len()
    // }

    // // Similarly, implement this based on your game's structure
    // fn get_other_dynamic_elements_count(&self) -> usize {
    //     // For example, counting items or interactive objects
    //     0 // Placeholder, adjust as necessary
    // }

    // fn sprite_count(&self) -> usize {
    //     self.level.sprite_count()
    // }

    fn render(&mut self, frend: &mut Renderer) {
        let total_sprites_needed = self.calculate_total_sprites_needed();

        frend.sprite_group_resize(0, total_sprites_needed);

        self.level.render_into(frend, 0);

        let (sprite_posns, sprite_gfx) = frend.sprites_mut(0, 0..total_sprites_needed);

        let player_sprite_index = self.level.sprite_count();

        if let Some(player_sprite) = sprite_posns.get_mut(player_sprite_index) {
            player_sprite.x = (self.player.pos.x as f32) * TILE_SZ as f32 + TILE_SZ as f32 / 2.0;
            player_sprite.y = ((self.level.grid_height() as f32) - self.player.pos.y as f32)
                * TILE_SZ as f32
                - TILE_SZ as f32 / 2.0;
            player_sprite.w = (TILE_SZ as u16) / 2;
            player_sprite.h = (TILE_SZ as u16) / 2;
            player_sprite.rot = if self.is_player_alive { 0.0 } else { 90.0 };
        }

        if let Some(player_sprite_gfx) = sprite_gfx.get_mut(player_sprite_index) {
            *player_sprite_gfx = PLAYER;
        }

        // for (index, enemy) in self.enemies.iter().enumerate() {
        //     let sprite_index = self.level.sprite_count() + 1 + index;
        //     if let Some(enemy_sprite) = sprite_posns.get_mut(sprite_index) {
        //         enemy_sprite.x = (enemy.pos.x as f32) * TILE_SZ as f32 + TILE_SZ as f32 / 2.0;
        //         enemy_sprite.y = ((self.level.grid_height() as f32) - enemy.pos.y as f32)
        //             * TILE_SZ as f32
        //             - TILE_SZ as f32 / 2.0;
        //         enemy_sprite.w = TILE_SZ as u16;
        //         enemy_sprite.h = TILE_SZ as u16;
        //         enemy_sprite.rot = 0.0;
        //     }

        //     if let Some(enemy_sprite_gfx) = sprite_gfx.get_mut(sprite_index) {
        //         *enemy_sprite_gfx = ENEMY;
        //     }
        // }

        for (index, gold_pos) in self.golds.iter().enumerate() {
            let sprite_index = self.level.sprite_count() + index + 1; // Adjust index based on total_sprites_needed calculation
            if let Some(gold_sprite) = sprite_posns.get_mut(sprite_index) {
                gold_sprite.x = (gold_pos.x as f32) * TILE_SZ as f32 + TILE_SZ as f32 / 2.0;
                gold_sprite.y = ((self.level.grid_height() as f32) - gold_pos.y as f32) * TILE_SZ as f32 - TILE_SZ as f32 / 2.0;
                gold_sprite.w = TILE_SZ as u16 / 3;
                gold_sprite.h = TILE_SZ as u16 / 3;
                gold_sprite.rot = 0.0;
            }
    
            if let Some(gold_sprite_gfx) = sprite_gfx.get_mut(sprite_index) {
                *gold_sprite_gfx = GOLD;
            }
        }
    }

    fn simulate(&mut self, input: &Input, dt: f32) {
        if self.is_player_alive {

            let speed = 5.0;

            let dx = input.key_axis(Key::ArrowLeft, Key::ArrowRight);
            let dy = input.key_axis(Key::ArrowUp, Key::ArrowDown);

            self.player.pos.x += dx as f32 * speed * dt;
            self.player.pos.y += dy as f32 * speed * dt;

            let enemy_dt: f32 = 1.0 / 2.0;
            let enemy_speed = 2.0;

            self.frame_counter += 1;
            for enemy in &mut self.enemies {

                // make enemy point towards player
                let difference_in_x = self.player.pos.x - enemy.pos.x;
                let difference_in_y = self.player.pos.y - enemy.pos.y;

                // turn towards player, x direction
                if difference_in_x > 0.0 {
                    enemy.dir.x = 1.0;
                } else if difference_in_x < 0.0 {
                    enemy.dir.x = -1.0;
                } else {
                    enemy.dir.x = 0.0;
                }
                // turn towards player, y direction
                if difference_in_y > 0.0 {
                    enemy.dir.y = 1.0;
                } else if difference_in_y < 0.0 {
                    enemy.dir.y = -1.0;
                } else {
                    enemy.dir.y = 0.0;
                }

                let new_x = (enemy.pos.x as f32 + enemy.dir.x as f32 * enemy_speed * enemy_dt)

                    .round() as f32;
                let new_y = (enemy.pos.y as f32 + enemy.dir.y as f32 * enemy_speed * enemy_dt)
                    .round() as f32;
                if new_x >= 0.0
                    && new_x < self.level.grid_width() as f32
                    && new_y >= 0.0
                    && new_y < self.level.grid_height() as f32
                {
                    let dest = Vec2 { x: new_x, y: new_y };
                    if let Some(tile) = self.level.get_tile(dest) {
                        if !tile.solid {
                            enemy.pos.x = new_x as f32;
                            enemy.pos.y = new_y as f32;
                        } else {
                            enemy.dir.x = -enemy.dir.x;
                            enemy.dir.y = -enemy.dir.y;
                        }
                    }
                } else {
                    enemy.dir.x = -enemy.dir.x;
                    enemy.dir.y = -enemy.dir.y;
                }
            }
            self.frame_counter = 0;

            self.update_gold();

            // for enemy in &self.enemies {
            //     if self.player.pos == enemy.pos {
            //         self.is_player_alive = false;
            //         self.death_time = Some(std::time::Instant::now());
            //         break;
            //     }
            // }
        }
    }
}
