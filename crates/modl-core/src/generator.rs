use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::{
    AssetKind, CartridgeError, WorkcartProject, WorkcartRecipe, encode_workcart, parse_workcart,
};

const MAX_WFC_CELLS: usize = 4_096;
const MAX_WFC_TILES: usize = 256;
const WFC_RESTARTS: u64 = 8;

/// Materializes one deterministic recipe into its declared map asset and returns canonical M01W.
///
/// Recipes are author-time source: the generated map replaces the selected output payload while
/// the recipe itself remains in the work-cart for future regeneration.
///
/// # Errors
///
/// Returns an `M014027` diagnostic for an invalid generator input or unsatisfied WFC corpus.
pub fn materialize_workcart_recipe(
    source: &str,
    recipe_id: &str,
) -> Result<String, CartridgeError> {
    let project = parse_workcart(source)?;
    let recipe = project
        .recipes
        .iter()
        .find(|recipe| recipe.id() == recipe_id)
        .cloned()
        .ok_or_else(|| generator_error(format!("recipe '{recipe_id}' does not exist")))?;
    let payload = materialize_map(&project, &recipe)?;
    let output_path = project
        .manifest
        .assets
        .get(recipe.output())
        .ok_or_else(|| generator_error(format!("recipe '{}' has no output asset", recipe.id())))?
        .path
        .clone();
    let mut files = project.files.clone();
    files.insert(output_path, payload);
    encode_workcart(&project.manifest, &files, &project.recipes, &project.tests)
}

fn materialize_map(
    project: &WorkcartProject,
    recipe: &WorkcartRecipe,
) -> Result<Vec<u8>, CartridgeError> {
    let map = match recipe {
        WorkcartRecipe::Noise {
            seed,
            width,
            height,
            tile_set,
            tiles,
            ..
        } => {
            let tile_count = read_tile_set(project, tile_set)?;
            for tile in tiles {
                require_tile(*tile, tile_count, recipe.id())?;
            }
            let cells = generate_noise(*seed, *width, *height, tiles);
            generated_map(*width, *height, cells, tile_set)
        }
        WorkcartRecipe::Wfc {
            seed,
            width,
            height,
            tile_set,
            source_map,
            pattern_size,
            ..
        } => {
            let tile_count = read_tile_set(project, tile_set)?;
            let source = read_map(project, source_map)?;
            let layer = source
                .layers
                .iter()
                .find(|layer| layer.tile_set == *tile_set)
                .ok_or_else(|| {
                    generator_error(format!(
                        "wfc recipe '{}' source map '{source_map}' has no layer using tile set '{tile_set}'",
                        recipe.id()
                    ))
                })?;
            let cells = generate_wfc(
                *seed,
                usize::from(*width),
                usize::from(*height),
                usize::from(*pattern_size),
                layer,
                tile_count,
                recipe.id(),
            )?;
            generated_map(*width, *height, cells, tile_set)
        }
    };
    serde_json::to_vec(&map)
        .map_err(|error| generator_error(format!("could not encode generated map: {error}")))
}

fn generate_noise(seed: u64, width: u16, height: u16, tiles: &[u16]) -> Vec<u16> {
    let mut cells = Vec::with_capacity(usize::from(width) * usize::from(height));
    for y in 0..height {
        for x in 0..width {
            let coordinate = (u64::from(y) << 32) | u64::from(x);
            let index = random_index(mix(seed ^ coordinate), tiles.len());
            cells.push(tiles[index]);
        }
    }
    cells
}

fn generate_wfc(
    seed: u64,
    width: usize,
    height: usize,
    pattern_size: usize,
    layer: &MapLayer,
    tile_count: usize,
    recipe_id: &str,
) -> Result<Vec<u16>, CartridgeError> {
    let output_cells = width * height;
    if output_cells > MAX_WFC_CELLS {
        return Err(generator_error(format!(
            "wfc recipe '{recipe_id}' exceeds the {MAX_WFC_CELLS}-cell generation limit"
        )));
    }
    if usize::from(layer.width) < pattern_size || usize::from(layer.height) < pattern_size {
        return Err(generator_error(format!(
            "wfc recipe '{recipe_id}' pattern_size {pattern_size} exceeds its source-map dimensions"
        )));
    }
    for tile in &layer.cells {
        require_tile(*tile, tile_count, recipe_id)?;
    }
    let tiles: Vec<_> = layer
        .cells
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if tiles.is_empty() || tiles.len() > MAX_WFC_TILES {
        return Err(generator_error(format!(
            "wfc recipe '{recipe_id}' needs between one and {MAX_WFC_TILES} source tiles"
        )));
    }
    let tile_indexes: BTreeMap<_, _> = tiles
        .iter()
        .copied()
        .enumerate()
        .map(|(index, tile)| (tile, index))
        .collect();
    let compatibility = learn_compatibility(layer, pattern_size, &tile_indexes)?;
    let solved = solve_wfc(width, height, &compatibility, tiles.len(), seed).ok_or_else(|| {
        generator_error(format!(
            "wfc recipe '{recipe_id}' constraints could not be satisfied in {WFC_RESTARTS} attempts"
        ))
    })?;
    Ok(solved.into_iter().map(|index| tiles[index]).collect())
}

fn learn_compatibility(
    layer: &MapLayer,
    pattern_size: usize,
    tile_indexes: &BTreeMap<u16, usize>,
) -> Result<[Vec<Domain>; 4], CartridgeError> {
    let mut right = vec![Domain::EMPTY; tile_indexes.len()];
    let mut down = vec![Domain::EMPTY; tile_indexes.len()];
    let source_width = usize::from(layer.width);
    let source_height = usize::from(layer.height);
    for top in 0..=source_height - pattern_size {
        for left in 0..=source_width - pattern_size {
            for y in 0..pattern_size {
                for x in 0..pattern_size {
                    let tile = source_cell(layer, left + x, top + y)?;
                    let index = *tile_indexes.get(&tile).ok_or_else(|| {
                        generator_error(
                            "source map tile index is missing from WFC corpus".to_owned(),
                        )
                    })?;
                    if x + 1 < pattern_size {
                        let neighbor = source_cell(layer, left + x + 1, top + y)?;
                        let neighbor_index = *tile_indexes.get(&neighbor).ok_or_else(|| {
                            generator_error(
                                "source map tile index is missing from WFC corpus".to_owned(),
                            )
                        })?;
                        right[index].insert(neighbor_index);
                    }
                    if y + 1 < pattern_size {
                        let neighbor = source_cell(layer, left + x, top + y + 1)?;
                        let neighbor_index = *tile_indexes.get(&neighbor).ok_or_else(|| {
                            generator_error(
                                "source map tile index is missing from WFC corpus".to_owned(),
                            )
                        })?;
                        down[index].insert(neighbor_index);
                    }
                }
            }
        }
    }
    let left = reverse_compatibility(&right);
    let up = reverse_compatibility(&down);
    Ok([right, left, down, up])
}

fn source_cell(layer: &MapLayer, x: usize, y: usize) -> Result<u16, CartridgeError> {
    layer
        .cells
        .get(y * usize::from(layer.width) + x)
        .copied()
        .ok_or_else(|| {
            generator_error("source map cells are inconsistent with its dimensions".to_owned())
        })
}

fn reverse_compatibility(forward: &[Domain]) -> Vec<Domain> {
    let mut reverse = vec![Domain::EMPTY; forward.len()];
    for (from, domain) in forward.iter().enumerate() {
        for (to, reverse_domain) in reverse.iter_mut().enumerate() {
            if domain.contains(to) {
                reverse_domain.insert(from);
            }
        }
    }
    reverse
}

fn solve_wfc(
    width: usize,
    height: usize,
    compatibility: &[Vec<Domain>; 4],
    tile_count: usize,
    seed: u64,
) -> Option<Vec<usize>> {
    let all_tiles = Domain::all(tile_count);
    for attempt in 0..WFC_RESTARTS {
        let mut domains = vec![all_tiles; width * height];
        if !propagate(
            &mut domains,
            width,
            height,
            compatibility,
            0..width * height,
        ) {
            continue;
        }
        let mut random = DeterministicRandom::new(seed.wrapping_add(attempt));
        let mut failed = false;
        while let Some(cell) = lowest_entropy_cell(&domains) {
            let candidates = domains[cell].values(tile_count);
            let choice = candidates[random_index(random.next(), candidates.len())];
            domains[cell] = Domain::single(choice);
            if !propagate(&mut domains, width, height, compatibility, [cell]) {
                failed = true;
                break;
            }
        }
        if !failed {
            return domains
                .iter()
                .map(|domain| domain.first(tile_count))
                .collect();
        }
    }
    None
}

fn lowest_entropy_cell(domains: &[Domain]) -> Option<usize> {
    domains
        .iter()
        .enumerate()
        .filter_map(|(index, domain)| {
            let count = domain.count();
            (count > 1).then_some((index, count))
        })
        .min_by_key(|(index, count)| (*count, *index))
        .map(|(index, _)| index)
}

fn propagate(
    domains: &mut [Domain],
    width: usize,
    height: usize,
    compatibility: &[Vec<Domain>; 4],
    initial: impl IntoIterator<Item = usize>,
) -> bool {
    let mut queue = initial.into_iter().collect::<VecDeque<_>>();
    while let Some(cell) = queue.pop_front() {
        let x = cell % width;
        let y = cell / width;
        let neighbors = [
            (0, (x + 1 < width).then(|| cell + 1)),
            (1, (x > 0).then(|| cell - 1)),
            (2, (y + 1 < height).then(|| cell + width)),
            (3, (y > 0).then(|| cell - width)),
        ];
        for (direction, neighbor) in neighbors {
            let Some(neighbor) = neighbor else {
                continue;
            };
            let supported = domains[cell].supported(&compatibility[direction]);
            let reduced = domains[neighbor].intersection(supported);
            if reduced.is_empty() {
                return false;
            }
            if reduced != domains[neighbor] {
                domains[neighbor] = reduced;
                queue.push_back(neighbor);
            }
        }
    }
    true
}

fn generated_map(width: u16, height: u16, cells: Vec<u16>, tile_set: &str) -> MapAsset {
    MapAsset {
        revision: 1,
        kind: "map".to_owned(),
        layers: vec![MapLayer {
            width,
            height,
            cells,
            tile_set: tile_set.to_owned(),
        }],
    }
}

fn read_map(project: &WorkcartProject, name: &str) -> Result<MapAsset, CartridgeError> {
    let asset = project
        .manifest
        .assets
        .get(name)
        .filter(|asset| asset.kind == AssetKind::Map)
        .ok_or_else(|| generator_error(format!("map asset '{name}' does not exist")))?;
    let payload = project
        .files
        .get(&asset.path)
        .ok_or_else(|| generator_error(format!("map asset '{name}' payload is missing")))?;
    let map: MapAsset = serde_json::from_slice(payload).map_err(|error| {
        generator_error(format!(
            "map asset '{name}' is not a valid map payload: {error}"
        ))
    })?;
    if map.revision != 1 || map.kind != "map" || map.layers.is_empty() || map.layers.len() > 8 {
        return Err(generator_error(format!(
            "map asset '{name}' is not a revision-1 map"
        )));
    }
    for layer in &map.layers {
        if layer.width == 0
            || layer.height == 0
            || layer.cells.len() != usize::from(layer.width) * usize::from(layer.height)
            || layer.tile_set.is_empty()
        {
            return Err(generator_error(format!(
                "map asset '{name}' has an invalid layer"
            )));
        }
    }
    Ok(map)
}

fn read_tile_set(project: &WorkcartProject, name: &str) -> Result<usize, CartridgeError> {
    let asset = project
        .manifest
        .assets
        .get(name)
        .filter(|asset| asset.kind == AssetKind::TileSet)
        .ok_or_else(|| generator_error(format!("tile set '{name}' does not exist")))?;
    let payload = project
        .files
        .get(&asset.path)
        .ok_or_else(|| generator_error(format!("tile set '{name}' payload is missing")))?;
    let tiles: TileSetAsset = serde_json::from_slice(payload).map_err(|error| {
        generator_error(format!(
            "tile set '{name}' is not a valid tile-set payload: {error}"
        ))
    })?;
    if tiles.revision != 2
        || tiles.kind != "tile_set"
        || tiles.tiles.is_empty()
        || tiles.tiles.len() > MAX_WFC_TILES
        || tiles.tiles.iter().any(|tile| tile.len() != 64)
        || tiles.flags.len() != tiles.tiles.len()
    {
        return Err(generator_error(format!(
            "tile set '{name}' is not a revision-2 tile set"
        )));
    }
    Ok(tiles.tiles.len())
}

fn require_tile(tile: u16, tile_count: usize, recipe_id: &str) -> Result<(), CartridgeError> {
    if usize::from(tile) < tile_count {
        Ok(())
    } else {
        Err(generator_error(format!(
            "recipe '{recipe_id}' references tile {tile}, but its tile set contains {tile_count} tiles"
        )))
    }
}

fn generator_error(message: String) -> CartridgeError {
    CartridgeError {
        code: "M014027",
        message,
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MapAsset {
    revision: u8,
    kind: String,
    layers: Vec<MapLayer>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MapLayer {
    width: u16,
    height: u16,
    cells: Vec<u16>,
    #[serde(rename = "tileSet")]
    tile_set: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TileSetAsset {
    revision: u8,
    kind: String,
    tiles: Vec<Vec<u8>>,
    flags: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Domain([u64; 4]);

impl Domain {
    const EMPTY: Self = Self([0; 4]);

    fn all(tile_count: usize) -> Self {
        let mut result = Self::EMPTY;
        for tile in 0..tile_count {
            result.insert(tile);
        }
        result
    }

    fn single(tile: usize) -> Self {
        let mut result = Self::EMPTY;
        result.insert(tile);
        result
    }

    fn insert(&mut self, tile: usize) {
        let word = tile / 64;
        let bit = tile % 64;
        if let Some(value) = self.0.get_mut(word) {
            *value |= 1_u64 << bit;
        }
    }

    fn contains(self, tile: usize) -> bool {
        let word = tile / 64;
        let bit = tile % 64;
        self.0
            .get(word)
            .is_some_and(|value| value & (1_u64 << bit) != 0)
    }

    fn intersection(self, other: Self) -> Self {
        Self(std::array::from_fn(|index| self.0[index] & other.0[index]))
    }

    fn union(&mut self, other: Self) {
        for index in 0..self.0.len() {
            self.0[index] |= other.0[index];
        }
    }

    fn is_empty(self) -> bool {
        self.0.iter().all(|word| *word == 0)
    }

    fn count(self) -> usize {
        self.0
            .iter()
            .map(|word| usize::try_from(word.count_ones()).unwrap_or_default())
            .sum()
    }

    fn values(self, tile_count: usize) -> Vec<usize> {
        (0..tile_count)
            .filter(|tile| self.contains(*tile))
            .collect()
    }

    fn first(self, tile_count: usize) -> Option<usize> {
        (0..tile_count).find(|tile| self.contains(*tile))
    }

    fn supported(self, compatibility: &[Domain]) -> Self {
        let mut result = Self::EMPTY;
        for (tile, domain) in compatibility.iter().enumerate() {
            if self.contains(tile) {
                result.union(*domain);
            }
        }
        result
    }
}

#[derive(Clone, Copy, Debug)]
struct DeterministicRandom(u64);

impl DeterministicRandom {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        mix(self.0)
    }
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn random_index(value: u64, length: usize) -> usize {
    let length = u64::try_from(length).unwrap_or(1);
    usize::try_from(value % length).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use crate::{materialize_workcart_recipe, parse_workcart};

    const CART: &str = r#"format = 2

[cartridge]
language = "MODL/1"
id = "generated"
title = "GENERATED"
author = "@gongahkia"
version = "0.1.0"
entry = "src/main.modl"
update_rate = 60

[[module]]
path = "src/main.modl"
source = "on draw:\n  clear(0)\n"

[[asset]]
name = "tiles"
kind = "tile_set"
path = "assets/tiles.m01g"
payload = '''{"revision":2,"kind":"tile_set","tiles":[[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]],"flags":[0,0]}'''

[[asset]]
name = "source"
kind = "map"
path = "assets/source.m01m"
payload = '''{"revision":1,"kind":"map","layers":[{"width":2,"height":2,"cells":[0,1,1,0],"tileSet":"tiles"}]}'''

[[asset]]
name = "world"
kind = "map"
path = "assets/world.m01m"
payload = '''{"revision":1,"kind":"map","layers":[]}'''

[[recipe]]
algorithm = "noise"
id = "noise"
output = "world"
seed = 7
width = 4
height = 3
tile_set = "tiles"
tiles = [0, 1]

[[recipe]]
algorithm = "wfc"
id = "wfc"
output = "world"
seed = 7
width = 4
height = 3
tile_set = "tiles"
source_map = "source"
pattern_size = 2
"#;

    #[test]
    fn noise_materialization_is_deterministic_and_retains_the_recipe() {
        let first = materialize_workcart_recipe(CART, "noise").expect("noise generates");
        let second = materialize_workcart_recipe(CART, "noise").expect("noise generates");
        assert_eq!(first, second);
        crate::pack_workcart(&first)
            .expect("generated source passes current asset/capacity admission");
        let project = parse_workcart(&first).expect("generated work-cart parses");
        assert_eq!(project.recipes.len(), 2);
        let map: serde_json::Value =
            serde_json::from_slice(&project.files["assets/world.m01m"]).expect("map is JSON");
        assert_eq!(map["layers"][0]["width"], 4);
        assert_eq!(map["layers"][0]["height"], 3);
        let cells = map["layers"][0]["cells"].as_array().expect("map cells");
        assert_eq!(cells.len(), 12);
        assert!(cells.iter().all(|cell| cell == 0 || cell == 1));
    }

    #[test]
    fn wfc_materialization_uses_source_patterns_deterministically() {
        let generated = materialize_workcart_recipe(CART, "wfc").expect("wfc generates");
        crate::pack_workcart(&generated).expect("WFC output packs through current admission");
        assert_eq!(
            generated,
            materialize_workcart_recipe(CART, "wfc").expect("wfc regenerates")
        );
        let project = parse_workcart(&generated).expect("generated work-cart parses");
        let map: serde_json::Value =
            serde_json::from_slice(&project.files["assets/world.m01m"]).expect("map is JSON");
        let cells = map["layers"][0]["cells"].as_array().expect("map cells");
        for y in 0..3 {
            for x in 0..4 {
                let cell = &cells[y * 4 + x];
                if x + 1 < 4 {
                    assert_ne!(cell, &cells[y * 4 + x + 1]);
                }
                if y + 1 < 3 {
                    assert_ne!(cell, &cells[(y + 1) * 4 + x]);
                }
            }
        }
    }

    #[test]
    fn generation_rejects_unknown_recipes_and_invalid_tile_indexes() {
        let obsolete = CART.replacen(
            "\"revision\":2,\"kind\":\"tile_set\"",
            "\"revision\":1,\"kind\":\"tile_set\"",
            1,
        );
        assert!(
            materialize_workcart_recipe(&obsolete, "noise")
                .unwrap_err()
                .message
                .contains("revision-2")
        );
        assert_eq!(
            materialize_workcart_recipe(CART, "missing")
                .expect_err("missing recipe fails")
                .code,
            "M014027"
        );
        let invalid = CART.replace("tiles = [0, 1]", "tiles = [2]");
        assert_eq!(
            materialize_workcart_recipe(&invalid, "noise")
                .expect_err("invalid tile fails")
                .code,
            "M014027"
        );
    }
}
