//! src-tauri/src/backend/io/rest/mappers/child_mapper.rs

use crate::backend::domain::models::child::{ActiveChild, Child as DomainChild};
use shared::{
    ActiveChildResponse, Child as SharedChild, ChildListResponse, SetActiveChildResponse,
    ChildResponse,
};
use anyhow::{Result, Context};
use chrono::{DateTime, NaiveDate, Utc};

/// Mapper to convert between shared Child DTOs and domain Child models.
pub struct ChildMapper;

impl ChildMapper {
    /// Converts a shared Child DTO to a domain Child model.
    pub fn to_domain(dto: SharedChild) -> Result<DomainChild> {
        let birthdate = NaiveDate::parse_from_str(&dto.birthdate, "%Y-%m-%d")
            .context("Failed to parse birthdate from shared DTO")?;
        let created_at = DateTime::parse_from_rfc3339(&dto.created_at)
            .context("Failed to parse created_at from shared DTO")?
            .with_timezone(&Utc);
        let updated_at = DateTime::parse_from_rfc3339(&dto.updated_at)
            .context("Failed to parse updated_at from shared DTO")?
            .with_timezone(&Utc);

        Ok(DomainChild {
            id: dto.id,
            name: dto.name,
            birthdate,
            created_at,
            updated_at,
        })
    }

    /// Converts a domain Child model to a shared Child DTO.
    pub fn to_dto(domain: DomainChild) -> SharedChild {
        SharedChild {
            id: domain.id,
            name: domain.name,
            birthdate: domain.birthdate.format("%Y-%m-%d").to_string(),
            created_at: domain.created_at.to_rfc3339(),
            updated_at: domain.updated_at.to_rfc3339(),
        }
    }

    pub fn to_active_child_dto(domain: ActiveChild) -> ActiveChildResponse {
        ActiveChildResponse {
            active_child: domain.child.map(Self::to_dto),
        }
    }

    pub fn to_child_list_dto(domain_children: Vec<DomainChild>) -> ChildListResponse {
        ChildListResponse {
            children: domain_children.into_iter().map(Self::to_dto).collect(),
        }
    }
    
    pub fn to_set_active_child_dto(domain: DomainChild) -> SetActiveChildResponse {
        SetActiveChildResponse {
            active_child: Self::to_dto(domain),
            success_message: "Active child has been set successfully.".to_string(),
        }
    }

    pub fn to_child_response_dto(domain: DomainChild, message: &str) -> ChildResponse {
        ChildResponse {
            child: Self::to_dto(domain),
            success_message: message.to_string(),
        }
    }
} 