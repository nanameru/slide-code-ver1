#[derive(Debug, Clone, Default)]
pub struct PlanItem {
    pub step: String,
    pub status: String,
}

#[derive(Debug, Clone, Default)]
pub struct PlanUpdate {
    pub explanation: Option<String>,
    pub items: Vec<PlanItem>,
}

pub fn update_plan(mut current: Vec<PlanItem>, explanation: Option<String>) -> PlanUpdate {
    // Stub: echo back
    PlanUpdate { explanation, items: current.drain(..).collect() }
}

