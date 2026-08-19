//! Lazy-load entity store for [`Company`], [`Building`], and [`CommercialBuilding`] sectors.
//!
//! The Python engine persists companies and buildings in many small JSON files
//! under `data/entities/` and `data/spatial_registry/`. This module provides a
//! trait-based interface that lets the Rust engine load one sector at a time
//! (or keep sectors in memory for tests) without deserializing the whole world
//! into RAM.

use crate::entities::{Building, Company};
use crate::society::housing::{CommercialBuilding, CommercialBuildingType, HousingBuilding};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Error type for entity-store load/save operations.
#[derive(Debug)]
pub enum EntityStoreError {
    /// The file could not be read from or written to disk.
    Io(std::io::Error),
    /// The file contents could not be parsed as the expected schema.
    Json(serde_json::Error),
    /// A building sector was requested without a region.
    MissingRegion(String),
}

impl fmt::Display for EntityStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntityStoreError::Io(e) => write!(f, "I/O error: {e}"),
            EntityStoreError::Json(e) => write!(f, "JSON error: {e}"),
            EntityStoreError::MissingRegion(e) => write!(f, "missing region: {e}"),
        }
    }
}

impl std::error::Error for EntityStoreError {}

impl From<std::io::Error> for EntityStoreError {
    fn from(e: std::io::Error) -> Self {
        EntityStoreError::Io(e)
    }
}

impl From<serde_json::Error> for EntityStoreError {
    fn from(e: serde_json::Error) -> Self {
        EntityStoreError::Json(e)
    }
}

/// Marks a type that can be loaded and saved by [`EntityStore`].
///
/// Implementors must be (de)serializable and must define the file path layout
/// for a given country, sector, and optional region.
pub trait Entity: Serialize + DeserializeOwned {
    /// Builds the full file path for the requested sector.
    ///
    /// # Arguments
    /// * `data_dir` - Root save directory (e.g. `data/`).
    /// * `country` - Country name.
    /// * `sector` - Sector file name without extension.
    /// * `region` - Region id, required for building sectors.
    fn path(
        data_dir: &Path,
        country: &str,
        sector: &str,
        region: Option<&str>,
    ) -> Result<PathBuf, EntityStoreError>;
}

impl Entity for Company {
    fn path(
        data_dir: &Path,
        country: &str,
        sector: &str,
        _region: Option<&str>,
    ) -> Result<PathBuf, EntityStoreError> {
        Ok(data_dir
            .join("entities")
            .join(country)
            .join("companies")
            .join(format!("{sector}.json")))
    }
}

impl Entity for Building {
    fn path(
        data_dir: &Path,
        country: &str,
        sector: &str,
        region: Option<&str>,
    ) -> Result<PathBuf, EntityStoreError> {
        let region = region.ok_or_else(|| {
            EntityStoreError::MissingRegion(format!(
                "building sector {sector} requires a region for country {country}"
            ))
        })?;
        Ok(data_dir
            .join("spatial_registry")
            .join(country)
            .join(region)
            .join("buildings")
            .join(format!("{sector}.json")))
    }
}

impl Entity for CommercialBuilding {
    fn path(
        data_dir: &Path,
        country: &str,
        sector: &str,
        _region: Option<&str>,
    ) -> Result<PathBuf, EntityStoreError> {
        Ok(data_dir
            .join("entities")
            .join(country)
            .join("commercial")
            .join(format!("{sector}.json")))
    }
}

impl Entity for HousingBuilding {
    fn path(
        data_dir: &Path,
        country: &str,
        sector: &str,
        _region: Option<&str>,
    ) -> Result<PathBuf, EntityStoreError> {
        Ok(data_dir
            .join("entities")
            .join(country)
            .join("housing")
            .join(format!("{sector}.json")))
    }
}

/// Storage backend for a single entity type.
///
/// # Type Parameters
/// * `T` - The entity type (`Company` or `Building`).
///
/// # Rules
/// * `load_sector` returns all entities stored in one JSON file.
/// * `save_sector` overwrites that file with the supplied slice.
/// * Sectors are identified by `country`, `sector` and an optional `region`.
pub trait EntityStore<T: Entity + Clone> {
    /// Loads one sector file as a vector of entities.
    fn load_sector(
        &self,
        country: &str,
        sector: &str,
        region: Option<&str>,
    ) -> Result<Vec<T>, EntityStoreError>;

    /// Saves a vector of entities into one sector file.
    fn save_sector(
        &self,
        country: &str,
        sector: &str,
        region: Option<&str>,
        entities: &[T],
    ) -> Result<(), EntityStoreError>;
}

/// Disk-backed entity store.
///
/// # Rules
/// * `data_dir` is the root save directory.
/// * Files are read/written as pretty-printed JSON arrays.
#[derive(Debug, Clone)]
pub struct DiskEntityStore<T> {
    data_dir: PathBuf,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> DiskEntityStore<T> {
    /// Creates a new disk-backed store rooted at `data_dir`.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T: Entity + Clone> EntityStore<T> for DiskEntityStore<T> {
    fn load_sector(
        &self,
        country: &str,
        sector: &str,
        region: Option<&str>,
    ) -> Result<Vec<T>, EntityStoreError> {
        let path = T::path(&self.data_dir, country, sector, region)?;
        let text = fs::read_to_string(&path)?;
        let entities = serde_json::from_str(&text)?;
        Ok(entities)
    }

    fn save_sector(
        &self,
        country: &str,
        sector: &str,
        region: Option<&str>,
        entities: &[T],
    ) -> Result<(), EntityStoreError> {
        let path = T::path(&self.data_dir, country, sector, region)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(entities)?;
        fs::write(&path, text)?;
        Ok(())
    }
}

/// In-memory entity store for tests and isolated simulations.
///
/// # Rules
/// * Stores sectors in a `HashMap` keyed by `country/sector/region`.
/// * A missing sector returns an empty vector.
#[derive(Debug, Clone)]
pub struct MemoryEntityStore<T: Clone> {
    store: RefCell<HashMap<String, Vec<T>>>,
}

impl<T: Clone> Default for MemoryEntityStore<T> {
    fn default() -> Self {
        Self {
            store: RefCell::new(HashMap::new()),
        }
    }
}

impl<T: Clone> MemoryEntityStore<T> {
    /// Creates an empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }

    fn key(country: &str, sector: &str, region: Option<&str>) -> String {
        format!("{}|{}|{}", country, sector, region.unwrap_or("-"))
    }
}

impl<T: Entity + Clone> EntityStore<T> for MemoryEntityStore<T> {
    fn load_sector(
        &self,
        country: &str,
        sector: &str,
        region: Option<&str>,
    ) -> Result<Vec<T>, EntityStoreError> {
        // Path validation is still useful for buildings.
        let _ = T::path(Path::new("."), country, sector, region)?;
        let key = Self::key(country, sector, region);
        Ok(self.store.borrow().get(&key).cloned().unwrap_or_default())
    }

    fn save_sector(
        &self,
        country: &str,
        sector: &str,
        region: Option<&str>,
        entities: &[T],
    ) -> Result<(), EntityStoreError> {
        let _ = T::path(Path::new("."), country, sector, region)?;
        let key = Self::key(country, sector, region);
        self.store.borrow_mut().insert(key, entities.to_vec());
        Ok(())
    }
}
