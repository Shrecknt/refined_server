use azalea::prelude::*;
use lazy_static::lazy_static;
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Component)]
pub struct Config {
    pub bot_owner: String,
    pub region: Region,
    pub depot: Depot,
}
#[derive(Deserialize, Debug, Clone)]
pub struct Region {
    pub walking_level: i32,
    pub x1: i32,
    pub z1: i32,
    pub x2: i32,
    pub z2: i32,
    pub min_y: i32,
    pub max_y: i32,
}
#[derive(Deserialize, Debug, Clone)]
pub struct Depot {
    pub storage_x: i32,
    pub storage_y: i32,
    pub storage_z: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

lazy_static! {
    pub static ref CONFIG: Config =
        toml::from_str(&std::fs::read_to_string("config.toml").unwrap()).unwrap();
}
