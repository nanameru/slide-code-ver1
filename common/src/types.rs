use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideInfo {
    pub title: String,
    pub content: String,
    pub slide_number: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideRequest {
    pub prompt: String,
    pub num_slides: usize,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideResponse {
    pub markdown: String,
}