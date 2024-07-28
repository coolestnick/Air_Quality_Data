#[macro_use]
extern crate serde;
use candid::{Decode, Encode};
use ic_cdk::api::time;
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{BoundedStorable, Cell, DefaultMemoryImpl, StableBTreeMap, Storable};
use std::collections::HashMap;
use std::{borrow::Cow, cell::RefCell};

type Memory = VirtualMemory<DefaultMemoryImpl>;
type IdCell = Cell<u64, Memory>;

#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
struct AirQualityData {
    id: u64,
    location: String,
    timestamp: u64,
    air_quality_index: u32,
    health_recommendations: String,
    pollutant_levels: HashMap<String, f64>,
    weather_conditions: WeatherData,
}

impl Storable for AirQualityData {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

impl BoundedStorable for AirQualityData {
    const MAX_SIZE: u32 = 1024;
    const IS_FIXED_SIZE: bool = false;
}

#[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
struct WeatherData {
    temperature: f64,
    humidity: f64,
    wind_speed: f64,
}

thread_local! {
    static AIR_QUALITY_MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(
        MemoryManager::init(DefaultMemoryImpl::default())
    );

    static AIR_QUALITY_ID_COUNTER: RefCell<IdCell> = RefCell::new(
        IdCell::init(AIR_QUALITY_MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0))), 0)
            .expect("Cannot create a counter for air quality data")
    );

    static AIR_QUALITY_STORAGE: RefCell<StableBTreeMap<u64, AirQualityData, Memory>> =
        RefCell::new(StableBTreeMap::init(
            AIR_QUALITY_MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(1)))
    ));
}

fn do_insert_air_quality(data: &AirQualityData) {
    AIR_QUALITY_STORAGE.with(|service| service.borrow_mut().insert(data.id, data.clone()));
}

#[derive(candid::CandidType, Serialize, Deserialize, Default)]
struct AirQualityUpdatePayload {
    location: String,
    air_quality_index: u32,
    health_recommendations: String,
    pollutant_levels: Option<HashMap<String, f64>>,
    weather_conditions: Option<WeatherData>,
}

#[ic_cdk::query]
fn get_air_quality_data(id: u64) -> Result<AirQualityData, Error> {
    match _get_air_quality_data(&id) {
        Some(data) => Ok(data),
        None => Err(Error::NotFound {
            msg: format!("air quality data with id={} not found", id),
        }),
    }
}

fn _get_air_quality_data(id: &u64) -> Option<AirQualityData> {
    AIR_QUALITY_STORAGE.with(|s| s.borrow().get(id))
}

#[ic_cdk::update]
fn add_air_quality_data(data: AirQualityUpdatePayload) -> Option<AirQualityData> {
    let id = AIR_QUALITY_ID_COUNTER
        .with(|counter| {
            let current_value = *counter.borrow().get();
            counter.borrow_mut().set(current_value + 1)
        })
        .expect("cannot increment id counter for air quality data");

    let pollutant_levels = data.pollutant_levels.unwrap_or_default();
    let weather_conditions = data.weather_conditions.unwrap_or_default();

    let air_quality_data = AirQualityData {
        id,
        location: data.location,
        timestamp: time(),
        air_quality_index: data.air_quality_index,
        health_recommendations: data.health_recommendations,
        pollutant_levels,
        weather_conditions,
    };

    do_insert_air_quality(&air_quality_data);
    Some(air_quality_data)
}

#[ic_cdk::update]
fn update_air_quality_data(
    id: u64,
    payload: AirQualityUpdatePayload,
) -> Result<AirQualityData, Error> {
    match AIR_QUALITY_STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut data) => {
            data.location = payload.location;
            data.air_quality_index = payload.air_quality_index;
            data.health_recommendations = payload.health_recommendations;
            data.pollutant_levels = payload.pollutant_levels.unwrap_or_default();
            data.weather_conditions = payload.weather_conditions.unwrap_or_default();
            data.timestamp = time();

            do_insert_air_quality(&data);
            Ok(data)
        }
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't update air quality data with id={}. data not found",
                id
            ),
        }),
    }
}

#[ic_cdk::update]
fn delete_air_quality_data(id: u64) -> Result<AirQualityData, Error> {
    match AIR_QUALITY_STORAGE.with(|service| service.borrow_mut().remove(&id)) {
        Some(data) => Ok(data),
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't delete air quality data with id={}. data not found.",
                id
            ),
        }),
    }
}

#[ic_cdk::query]
fn get_all_air_quality_data() -> Result<Vec<AirQualityData>, Error> {
    Ok(AIR_QUALITY_STORAGE.with(|service| {
        let storage = service.borrow_mut();
        storage.iter().map(|(_, item)| item.clone()).collect()
    }))
}

#[ic_cdk::query]
fn search_air_quality_data_by_location(location: String) -> Result<Vec<AirQualityData>, Error> {
    Ok(AIR_QUALITY_STORAGE.with(|service| {
        let borrow = &*service.borrow();
        borrow
            .iter()
            .filter_map(|(_, space)| {
                if space.location.contains(&location) {
                    Some(space.clone())
                } else {
                    None
                }
            })
            .collect()
    }))
}

#[ic_cdk::query]
fn get_air_quality_data_by_weather_conditions(
    min_temperature: f64,
    max_temperature: f64,
    min_humidity: f64,
    max_humidity: f64,
    min_wind_speed: f64,
    max_wind_speed: f64,
) -> Result<Vec<AirQualityData>, Error> {
    Ok(AIR_QUALITY_STORAGE.with(|service| {
        let borrow = service.borrow();
        borrow
            .iter()
            .filter_map(|(_, data)| {
                let weather = &data.weather_conditions;
                if weather.temperature >= min_temperature
                    && weather.temperature <= max_temperature
                    && weather.humidity >= min_humidity
                    && weather.humidity <= max_humidity
                    && weather.wind_speed >= min_wind_speed
                    && weather.wind_speed <= max_wind_speed
                {
                    Some(data.clone())
                } else {
                    None
                }
            })
            .collect()
    }))
}

#[ic_cdk::query]
fn get_air_quality_data_by_pollutant_level(
    pollutant: String,
    min_level: f64,
    max_level: f64,
) -> Result<Vec<AirQualityData>, Error> {
    Ok(AIR_QUALITY_STORAGE.with(|service| {
        let borrow = service.borrow();
        borrow
            .iter()
            .filter_map(|(_, data)| {
                if let Some(level) = data.pollutant_levels.get(&pollutant) {
                    if *level >= min_level && *level <= max_level {
                        Some(data.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }))
}

#[ic_cdk::query]
fn get_air_quality_data_by_timestamp_range(
    start_timestamp: u64,
    end_timestamp: u64,
) -> Result<Vec<AirQualityData>, Error> {
    Ok(AIR_QUALITY_STORAGE.with(|service| {
        let borrow = service.borrow();
        borrow
            .iter()
            .filter_map(|(_, data)| {
                if data.timestamp >= start_timestamp && data.timestamp <= end_timestamp {
                    Some(data.clone())
                } else {
                    None
                }
            })
            .collect()
    }))
}

// New Functions

#[ic_cdk::query]
fn get_recent_air_quality_data(n: usize) -> Result<Vec<AirQualityData>, Error> {
    let mut all_data = AIR_QUALITY_STORAGE.with(|service| {
        let storage = service.borrow();
        storage.iter().map(|(_, data)| data.clone()).collect::<Vec<_>>()
    });

    all_data.sort_by_key(|data| -(data.timestamp as i64)); // Sort by timestamp descending
    Ok(all_data.into_iter().take(n).collect())
}

#[ic_cdk::query]
fn get_average_air_quality_index(location: String) -> Result<f64, Error> {
    let data = AIR_QUALITY_STORAGE.with(|service| {
        service
            .borrow()
            .iter()
            .filter_map(|(_, data)| {
                if data.location == location {
                    Some(data.air_quality_index)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    });

    if data.is_empty() {
        return Err(Error::NotFound {
            msg: format!("No data found for location {}", location),
        });
    }

    let sum: u32 = data.iter().sum();
    let average = sum as f64 / data.len() as f64;
    Ok(average)
}

#[ic_cdk::query]
fn get_health_recommendations(air_quality_index: u32) -> String {
    match air_quality_index {
        0..=50 => "Air quality is good. No health implications.".to_string(),
        51..=100 => "Air quality is moderate. People with respiratory or heart issues should limit outdoor activities.".to_string(),
        101..=150 => "Air quality is unhealthy for sensitive groups. Limit outdoor activities if you have health issues.".to_string(),
        151..=200 => "Air quality is unhealthy. Everyone should limit prolonged outdoor exertion.".to_string(),
        201..=300 => "Air quality is very unhealthy. Health warnings of emergency conditions.".to_string(),
        _ => "Air quality is hazardous. Everyone should avoid all outdoor activities.".to_string(),
    }
}

#[ic_cdk::update]
fn delete_air_quality_data_by_location(location: String) -> Result<u64, Error> {
    let ids_to_delete = AIR_QUALITY_STORAGE.with(|service| {
        let storage = service.borrow();
        storage
            .iter()
            .filter_map(|(id, data)| if data.location == location { Some(id) } else { None })
            .collect::<Vec<_>>()
    });

    let count = ids_to_delete.len() as u64;
    if count == 0 {
        return Err(Error::NotFound {
            msg: format!("No data found for location {}", location),
        });
    }

    AIR_QUALITY_STORAGE.with(|service| {
        let mut storage = service.borrow_mut();
        for id in ids_to_delete {
            storage.remove(&id);
        }
    });

    Ok(count)
}


#[derive(candid::CandidType, Deserialize, Serialize)]
enum Error {
    NotFound { msg: String },
}

ic_cdk::export_candid!();