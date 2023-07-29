use std::error::Error;
use std::result;

use serde::{Serialize, Deserialize};
use handlebars::Handlebars;

const REVIEW_START_TEMPLATE: &str = r#"<orbit-reviewarea>"#;
const PROMPT_TEMPLATE: &str = r#"<orbit-prompt question="{{question}}" question-attachments="{{question-attachments}}" answer="{{answer}}"></orbit-prompt>"#;
const REVIEW_END: &str = "</orbit-reviewarea>";

type Result<T> = result::Result<T, Box<dyn Error>>;

#[derive(Serialize, Deserialize, Debug)]
pub struct Orbit {
    deck: Vec<OrbitCard>
}

impl Orbit {
    pub fn to_html(&self) -> Result<String> {
        let mut review = String::from(REVIEW_START_TEMPLATE);
        for card in &self.deck {
            let card_as_html = card.to_html()?;
            review.push_str(&card_as_html);
        }
        review.push_str(REVIEW_END);

        return Ok(review);
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OrbitCard {
    question: String,
    question_attachments: String,
    answer: String,
}

impl OrbitCard {
    pub fn to_html(&self) -> Result<String> {
        let card_map = &serde_json::json!({
            "question": self.question,
            "question-attachments": self.question_attachments,
            "answer": self.answer
        });


        let mut register = Handlebars::new();
        register.register_escape_fn(handlebars::no_escape);
        let render = register.render_template(PROMPT_TEMPLATE, card_map)?;

        return Ok(render);
    }
}
