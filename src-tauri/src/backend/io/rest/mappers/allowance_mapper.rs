//! Mappers for converting between allowance domain models and shared DTOs.

use crate::backend::domain::models::allowance::AllowanceConfig as DomainAllowanceConfig;
use shared::AllowanceConfig as SharedAllowanceConfig;

pub struct AllowanceMapper;

impl AllowanceMapper {
    pub fn to_dto(domain: DomainAllowanceConfig) -> SharedAllowanceConfig {
        SharedAllowanceConfig {
            id: domain.id,
            child_id: domain.child_id,
            amount: domain.amount,
            day_of_week: domain.day_of_week,
            is_active: domain.is_active,
            created_at: domain.created_at,
            updated_at: domain.updated_at,
        }
    }

    pub fn to_domain(dto: SharedAllowanceConfig) -> DomainAllowanceConfig {
        DomainAllowanceConfig {
            id: dto.id,
            child_id: dto.child_id,
            amount: dto.amount,
            day_of_week: dto.day_of_week,
            is_active: dto.is_active,
            created_at: dto.created_at,
            updated_at: dto.updated_at,
        }
    }
} 