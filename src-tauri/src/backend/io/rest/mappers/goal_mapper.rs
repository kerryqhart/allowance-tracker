use shared::{
    Goal, GoalCalculation, CreateGoalRequest, CreateGoalResponse, UpdateGoalRequest,
    UpdateGoalResponse, GetCurrentGoalRequest, GetCurrentGoalResponse, GetGoalHistoryRequest,
    GetGoalHistoryResponse, CancelGoalRequest, CancelGoalResponse, GoalState,
};
use crate::backend::domain::models::goal::{DomainGoal, DomainGoalState};

pub struct GoalMapper;

impl GoalMapper {
    /// Convert shared GoalState to domain DomainGoalState
    pub fn goal_state_to_domain(dto_state: GoalState) -> DomainGoalState {
        match dto_state {
            GoalState::Active => DomainGoalState::Active,
            GoalState::Cancelled => DomainGoalState::Cancelled,
            GoalState::Completed => DomainGoalState::Completed,
        }
    }

    /// Convert domain DomainGoalState to shared GoalState
    pub fn goal_state_to_dto(domain_state: DomainGoalState) -> GoalState {
        match domain_state {
            DomainGoalState::Active => GoalState::Active,
            DomainGoalState::Cancelled => GoalState::Cancelled,
            DomainGoalState::Completed => GoalState::Completed,
        }
    }

    /// Convert shared Goal DTO to domain DomainGoal
    pub fn to_domain(dto: Goal) -> DomainGoal {
        DomainGoal {
            id: dto.id,
            child_id: dto.child_id,
            description: dto.description,
            target_amount: dto.target_amount,
            state: Self::goal_state_to_domain(dto.state),
            created_at: dto.created_at,
            updated_at: dto.updated_at,
        }
    }

    /// Convert domain DomainGoal to shared Goal DTO
    pub fn to_dto(domain: DomainGoal) -> Goal {
        Goal {
            id: domain.id,
            child_id: domain.child_id,
            description: domain.description,
            target_amount: domain.target_amount,
            state: Self::goal_state_to_dto(domain.state),
            created_at: domain.created_at,
            updated_at: domain.updated_at,
        }
    }

    /// Convert Vec<DomainGoal> to Vec<Goal>
    pub fn to_dto_list(domain_goals: Vec<DomainGoal>) -> Vec<Goal> {
        domain_goals.into_iter().map(Self::to_dto).collect()
    }

    /// Convert Vec<Goal> to Vec<DomainGoal>
    pub fn to_domain_list(dto_goals: Vec<Goal>) -> Vec<DomainGoal> {
        dto_goals.into_iter().map(Self::to_domain).collect()
    }

    /// Convert GetCurrentGoalResponse with domain goal to DTO response
    pub fn to_get_current_goal_response(
        domain_goal: Option<DomainGoal>,
        calculation: Option<GoalCalculation>,
    ) -> GetCurrentGoalResponse {
        GetCurrentGoalResponse {
            goal: domain_goal.map(Self::to_dto),
            calculation,
        }
    }

    /// Convert CreateGoalResponse with domain goal to DTO response
    pub fn to_create_goal_response(
        domain_goal: DomainGoal,
        calculation: GoalCalculation,
        success_message: String,
    ) -> CreateGoalResponse {
        CreateGoalResponse {
            goal: Self::to_dto(domain_goal),
            calculation,
            success_message,
        }
    }

    /// Convert UpdateGoalResponse with domain goal to DTO response
    pub fn to_update_goal_response(
        domain_goal: DomainGoal,
        calculation: GoalCalculation,
        success_message: String,
    ) -> UpdateGoalResponse {
        UpdateGoalResponse {
            goal: Self::to_dto(domain_goal),
            calculation,
            success_message,
        }
    }

    /// Convert CancelGoalResponse with domain goal to DTO response
    pub fn to_cancel_goal_response(
        domain_goal: DomainGoal,
        success_message: String,
    ) -> CancelGoalResponse {
        CancelGoalResponse {
            goal: Self::to_dto(domain_goal),
            success_message,
        }
    }

    /// Convert GetGoalHistoryResponse with domain goals to DTO response
    pub fn to_get_goal_history_response(domain_goals: Vec<DomainGoal>) -> GetGoalHistoryResponse {
        GetGoalHistoryResponse {
            goals: Self::to_dto_list(domain_goals),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_domain_goal() -> DomainGoal {
        DomainGoal {
            id: "goal::child1_1234567890".to_string(),
            child_id: "child1".to_string(),
            description: "New bike".to_string(),
            target_amount: 100.0,
            state: DomainGoalState::Active,
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    fn sample_shared_goal() -> Goal {
        Goal {
            id: "goal::child1_1234567890".to_string(),
            child_id: "child1".to_string(),
            description: "New bike".to_string(),
            target_amount: 100.0,
            state: GoalState::Active,
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_to_domain() {
        let shared_goal = sample_shared_goal();
        let domain_goal = GoalMapper::to_domain(shared_goal.clone());
        
        assert_eq!(domain_goal.id, shared_goal.id);
        assert_eq!(domain_goal.child_id, shared_goal.child_id);
        assert_eq!(domain_goal.description, shared_goal.description);
        assert_eq!(domain_goal.target_amount, shared_goal.target_amount);
        assert_eq!(domain_goal.state, GoalMapper::goal_state_to_domain(shared_goal.state));
        assert_eq!(domain_goal.created_at, shared_goal.created_at);
        assert_eq!(domain_goal.updated_at, shared_goal.updated_at);
    }

    #[test]
    fn test_to_dto() {
        let domain_goal = sample_domain_goal();
        let shared_goal = GoalMapper::to_dto(domain_goal.clone());
        
        assert_eq!(shared_goal.id, domain_goal.id);
        assert_eq!(shared_goal.child_id, domain_goal.child_id);
        assert_eq!(shared_goal.description, domain_goal.description);
        assert_eq!(shared_goal.target_amount, domain_goal.target_amount);
        assert_eq!(shared_goal.state, GoalMapper::goal_state_to_dto(domain_goal.state));
        assert_eq!(shared_goal.created_at, domain_goal.created_at);
        assert_eq!(shared_goal.updated_at, domain_goal.updated_at);
    }

    #[test]
    fn test_bidirectional_conversion() {
        let original_domain = sample_domain_goal();
        let converted_shared = GoalMapper::to_dto(original_domain.clone());
        let converted_back = GoalMapper::to_domain(converted_shared);
        
        assert_eq!(original_domain, converted_back);
    }

    #[test]
    fn test_list_conversions() {
        let domain_goals = vec![sample_domain_goal()];
        let shared_goals = GoalMapper::to_dto_list(domain_goals.clone());
        let converted_back = GoalMapper::to_domain_list(shared_goals);
        
        assert_eq!(domain_goals, converted_back);
    }
} 