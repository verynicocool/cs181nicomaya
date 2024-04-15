use crate::geom::*;
use crate::grid::Coord;
use crate::grid::Grid;
use crate::EntityType;
use crate::TileData;
use crate::TILE_SZ;
use frenderer::{
    sprites::{SheetRegion, Transform},
    Renderer,
};
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug)]
struct Tileset {
    tiles: Vec<TileData>,
}
impl std::ops::Index<usize> for Tileset {
    type Output = TileData;
    fn index(&self, index: usize) -> &Self::Output {
        &self.tiles[index]
    }
}

#[allow(dead_code)]
pub struct Level {
    name: String,
    grid: Grid<u8>,
    tileset: Tileset,
    starts: Vec<(EntityType, Vec2)>,
}

const WALL: SheetRegion = SheetRegion::new(0, 186, 186, 0, 18, 18);

impl Level {
    /*
    We'll read from an ad hoc format like this, where FLAGS is either S (solid) or O (open) but could be other stuff later:

    LEVELNAME W H
    ====
    SYM FLAGS X Y W H
    SYM FLAGS X Y W H
    SYM FLAGS X Y W H
    ====
    SYM SYM SYM SYM SYM
    SYM SYM SYM SYM SYM
    SYM SYM SYM SYM SYM
    SYM SYM SYM SYM SYM
    SYM SYM SYM SYM SYM
    ====
    player X Y
    enemy X Y
    enemy X Y
    enemy X Y
    door LEVELNAME TO-X TO-Y X Y
    you can add more types of thing if you want
    */
    pub fn from_str(s: &str) -> Self {
        enum State {
            Metadata,
            Legend,
            Map,
            Starts,
            Done,
        }
        impl State {
            fn next(self) -> Self {
                match self {
                    Self::Metadata => Self::Legend,
                    Self::Legend => Self::Map,
                    Self::Map => Self::Starts,
                    Self::Starts => Self::Done,
                    Self::Done => Self::Done,
                }
            }
        }
        let mut state = State::Metadata;
        let mut name = None;
        let mut dims = None;
        let mut legend: HashMap<String, (u8, TileData)> = std::collections::HashMap::new();
        let mut grid = vec![];
        let mut starts = vec![];
        for line in s.lines() {
            if line.is_empty() {
                continue;
            } else if line.chars().all(|c| c == '=') {
                state = state.next();
            } else {
                match state {
                    State::Metadata => {
                        assert_eq!(name, None, "Two metadata lines! in {line}");
                        let mut chunks = line.split_whitespace();
                        name = Some(
                            chunks
                                .next()
                                .expect("No name in metadata line {line}")
                                .to_string(),
                        );
                        dims = Some((
                            u16::from_str(chunks.next().expect("No width in metadata line {line}"))
                                .expect("Couldn't parse width as u16 in {line}"),
                            u16::from_str(
                                chunks.next().expect("No height in metadata line {line}"),
                            )
                            .expect("Couldn't parse height as u16 in {line}"),
                        ));
                    }
                    State::Legend => {
                        let mut chunks = line.split_whitespace();
                        let sym = chunks.next().expect("Couldn't get tile symbol in {line}");
                        assert!(!legend.contains_key(sym), "Symbol {sym} already in legend");
                        let flags = chunks
                            .next()
                            .expect("Couldn't get tile flags in {line}")
                            .to_lowercase();
                        assert!(flags == "o" || flags == "s", "The only valid flags are o(pen) or s(olid), but you could parse other kinds here in {line}");
                        let x =
                            u16::from_str(chunks.next().expect("No sheet x in legend line {line}"))
                                .expect("Couldn't parse sheet x as u16 in {line}");
                        let y =
                            u16::from_str(chunks.next().expect("No sheet y in legend line {line}"))
                                .expect("Couldn't parse sheet y as u16 in {line}");
                        let w =
                            i16::from_str(chunks.next().expect("No sheet w in legend line {line}"))
                                .expect("Couldn't parse sheet w as i16 in {line}");
                        let h =
                            i16::from_str(chunks.next().expect("No sheet h in legend line {line}"))
                                .expect("Couldn't parse sheet h as i16 in {line}");
                        let data = TileData {
                            solid: flags == "s",
                            sheet_region: SheetRegion::new(0, x, y, 16, w, h),
                        };
                        legend.insert(sym.to_string(), (legend.len() as u8, data));
                    }
                    State::Map => {
                        let old_len = grid.len();
                        grid.extend(line.split_whitespace().map(|sym| legend[sym].0));
                        assert_eq!(
                            old_len + dims.unwrap().0 as usize,
                            grid.len(),
                            "map line is too short: {line} for map dims {dims:?}"
                        );
                    }
                    State::Starts => {
                        let mut chunks = line.split_whitespace();
                        let etype = chunks
                            .next()
                            .expect("Couldn't get entity start type {line}");
                        let etype = match etype {
                            "player" => EntityType::Player,
                            "enemy" => EntityType::Enemy,
                            "door" => {
                                let to_room = chunks.next().expect("Couldn't get dest room {line}");
                                let to_x = u16::from_str(
                                    chunks.next().expect("No dest x coord in door line {line}"),
                                )
                                .expect("Couldn't parse x coord as u16 in {line}");
                                let to_y = u16::from_str(
                                    chunks.next().expect("No dest y coord in door line {line}"),
                                )
                                .expect("Couldn't parse y coord as u16 in {line}");
                                EntityType::Door(to_room.to_string(), to_x, to_y)
                            }
                            _ => panic!("Unrecognized entity type in {line}"),
                        };
                        let x =
                            u16::from_str(chunks.next().expect("No x coord in start line {line}"))
                                .expect("Couldn't parse x coord as u16 in {line}");
                        let y =
                            u16::from_str(chunks.next().expect("No y coord in start line {line}"))
                                .expect("Couldn't parse y coord as u16 in {line}");
                        starts.push((
                            etype,
                            Vec2 {
                                x: x.into(),
                                y: y.into(),
                            },
                        ));
                    }
                    State::Done => {
                        panic!("Unexpected file content after parsing finished in {line}")
                    }
                }
            }
        }
        assert_ne!(name, None);
        let name = name.unwrap();
        assert_ne!(dims, None);
        let (w, h) = dims.unwrap();
        assert!(!legend.is_empty());
        assert_eq!(grid.len(), w as usize * h as usize);
        let mut tiles: Vec<(u8, TileData)> = legend.into_values().collect();
        tiles.sort_by_key(|(num, _)| *num);
        Self {
            name: name.to_string(),
            grid: Grid::new(w as usize, h as usize, grid),
            tileset: Tileset {
                tiles: tiles.into_iter().map(|(_num, val)| val).collect(),
            },
            starts,
        }
    }
    pub fn grid_height(&self) -> usize {
        self.grid.height()
    }
    pub fn grid_width(&self) -> usize {
        self.grid.width()
    }
    pub fn sprite_count(&self) -> usize {
        self.grid.width() * self.grid.height()
    }
    pub fn get_open_spaces(&self) -> Vec<Coord> {
        let mut open_spaces = Vec::new();
        for y in 0..self.grid_height() {
            for x in 0..self.grid_width() {
                if let Some(tile) = self.get_tile(Vec2 {
                    x: x as f32,
                    y: y as f32,
                }) {
                    if !tile.solid {
                        open_spaces.push((x, y));
                    }
                }
            }
        }
        open_spaces
    }
    pub fn render_into(&self, frend: &mut Renderer, offset: usize) -> usize {
        let (trfs, uvs) = frend.sprites_mut(0, offset..offset + self.sprite_count());
        for (y, row) in self.grid.row_iter().enumerate() {
            for (x, &tile_index) in row.iter().enumerate() {
                let tile_data = &self.tileset[tile_index as usize];
                trfs[offset + x + y * self.grid.width()] = Transform {
                    x: (x as f32 * TILE_SZ as f32) + TILE_SZ as f32 / 2.0,
                    y: (y as f32 * TILE_SZ as f32) + TILE_SZ as f32 / 2.0,
                    w: TILE_SZ as u16,
                    h: TILE_SZ as u16,
                    rot: 0.0,
                };
                uvs[offset + x + y * self.grid.width()] = if tile_data.solid {
                    WALL
                } else {
                    tile_data.sheet_region
                };
            }
        }
        self.sprite_count()
    }

    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn starts(&self) -> &[(EntityType, Vec2)] {
        &self.starts
    }
    pub fn get_tile(&self, pos: Vec2) -> Option<&TileData> {
        self.grid
            .get(pos.x as usize, pos.y as usize)
            .map(|t| &self.tileset[*t as usize])
    }
}
