use tauri_plugin_http::reqwest::{Client, Response};

#[tauri::command]
pub async fn send_feedback(feedback: String, email: Option<String>) -> Result<(), String> {
    let uniq_id = uuid::Uuid::new_v4();

    let response = make_request(&serde_json::json!({
        "api_key": "phc_EOWZsUljQ4HdvlGgoVAhhjktfDDDqYf4lKxzZ1wDkJv",
        "event": "survey sent",
        "distinct_id": uniq_id.to_string(),
        "properties": {
            "$survey_id": "019983b0-154a-0000-890c-fb4ce8595d9b",
            "$survey_response_60d8aebe-4b75-4bac-8221-dcaf59a1377d": feedback,
            "$survey_response_8877868a-a25a-4dfe-ba34-842894c7444f": email.unwrap_or_default(),
            "$survey_questions": [
                {
                    "id": "60d8aebe-4b75-4bac-8221-dcaf59a1377d",
                    "question": "What would you like to share with us?",
                },
                {
                    "id": "8877868a-a25a-4dfe-ba34-842894c7444f",
                    "question": "Email (optional)",
                }
            ]
        },
    }))
    .await?;

    match response.error_for_status() {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

async fn make_request(body: &serde_json::Value) -> Result<Response, String> {
    let client = Client::new();
    client
        .post("https://us.i.posthog.com/i/v0/e/")
        .json(body)
        .send()
        .await
        .map_err(|e| e.to_string())
}
