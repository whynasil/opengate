use async_trait::async_trait;
use base64::Engine;
use futures::StreamExt;
use serde_json::Value;
use tokio::sync::Mutex;
use chromiumoxide::{Browser, BrowserConfig, Page};
use chromiumoxide::page::ScreenshotParams;

use crate::tools::{Tool, ToolDef, ToolOutput};

pub struct BrowserTool {
    browser: Mutex<Option<Browser>>,
    page: Mutex<Option<Page>>,
    chrome_path: String,
}

impl std::fmt::Debug for BrowserTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserTool")
            .field("chrome_path", &self.chrome_path)
            .finish()
    }
}

impl BrowserTool {
    pub fn new() -> Self {
        let chrome_path =
            std::env::var("CHROME_PATH").unwrap_or_else(|_| "/usr/bin/chromium".to_string());
        Self {
            browser: Mutex::new(None),
            page: Mutex::new(None),
            chrome_path,
        }
    }

    async fn ensure_browser(&self) -> Result<(), String> {
        let mut browser_lock = self.browser.lock().await;
        if browser_lock.is_some() {
            return Ok(());
        }

        let (browser, mut handler) = Browser::launch(
            BrowserConfig::builder()
                .chrome_executable(&self.chrome_path)
                .build()
                .map_err(|e| format!("Failed to build browser config: {}", e))?,
        )
        .await
        .map_err(|e| format!("Failed to launch browser: {}", e))?;

        tokio::spawn(async move {
            while let Some(_event) = handler.next().await {}
        });

        let page = browser
            .new_page("about:blank")
            .await
            .map_err(|e| format!("Failed to create page: {}", e))?;

        *browser_lock = Some(browser);
        *self.page.lock().await = Some(page);
        Ok(())
    }

    fn parse_ref(ref_str: &str) -> Result<usize, String> {
        let index = ref_str
            .strip_prefix('@')
            .and_then(|s| s.strip_prefix('e'))
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or_else(|| format!("Invalid element ref '{}': expected format @e1, @e2, etc.", ref_str))?;
        if index == 0 {
            return Err(format!("Invalid element ref '{}': index must be >= 1", ref_str));
        }
        Ok(index - 1)
    }

    async fn snapshot_page(&self, page: &Page) -> Result<String, String> {
        let js = r#"
            (function() {
                const selector = 'button, a, input, textarea, select, [tabindex], [role="button"], [role="link"], [role="textbox"], [role="combobox"], [role="checkbox"], [role="radio"], [contenteditable]';
                const els = document.querySelectorAll(selector);
                const items = [];
                els.forEach((el, i) => {
                    const ref = '@e' + (i + 1);
                    const tag = el.tagName.toLowerCase();
                    const type = el.type || '';
                    const label = el.getAttribute('aria-label') || el.placeholder || '';
                    const text = (el.textContent || '').trim().substring(0, 200) || el.value || '';
                    let desc = label || text || tag;
                    if (desc.length > 100) desc = desc.substring(0, 100) + '...';
                    const attrs = tag + (type ? '[type="' + type + '"]' : '');
                    items.push(ref + ' <' + attrs + '> ' + desc);
                });
                return items.length > 0 ? items.join('\n') : '(no interactive elements found)';
            })()
        "#;

        let result: String = page
            .evaluate_expression(js)
            .await
            .map_err(|e| format!("Snapshot JS failed: {}", e))?
            .into_value()
            .map_err(|e| format!("Failed to parse snapshot result: {}", e))?;

        Ok(result)
    }

    async fn action_navigate(&self, url: &str) -> Result<ToolOutput, String> {
        self.ensure_browser().await?;
        let page_guard = self.page.lock().await;
        let page = page_guard.as_ref().ok_or("Browser not initialized")?;

        page.goto(url)
            .await
            .map_err(|e| format!("Navigation to '{}' failed: {}", url, e))?;

        let snapshot = self.snapshot_page(page).await?;
        Ok(ToolOutput::Text(snapshot))
    }

    async fn action_snapshot(&self) -> Result<ToolOutput, String> {
        self.ensure_browser().await?;
        let page_guard = self.page.lock().await;
        let page = page_guard.as_ref().ok_or("Browser not initialized")?;

        let snapshot = self.snapshot_page(page).await?;
        Ok(ToolOutput::Text(snapshot))
    }

    async fn action_click(&self, ref_str: &str) -> Result<ToolOutput, String> {
        self.ensure_browser().await?;
        let page_guard = self.page.lock().await;
        let page = page_guard.as_ref().ok_or("Browser not initialized")?;

        let idx = Self::parse_ref(ref_str)?;

        let js = format!(
            r#"(function() {{
                const selector = 'button, a, input, textarea, select, [tabindex], [role="button"], [role="link"], [role="textbox"], [role="combobox"], [role="checkbox"], [role="radio"], [contenteditable]';
                const els = document.querySelectorAll(selector);
                const el = els[{idx}];
                if (el) {{ el.click(); return 'clicked'; }}
                return 'element not found (index {idx})';
            }})()"#,
            idx = idx
        );

        page.evaluate_expression(&js)
            .await
            .map_err(|e| format!("Click on {} failed: {}", ref_str, e))?;

        Ok(ToolOutput::Text(format!("Clicked {}", ref_str)))
    }

    async fn action_type(&self, ref_str: &str, text: &str) -> Result<ToolOutput, String> {
        self.ensure_browser().await?;
        let page_guard = self.page.lock().await;
        let page = page_guard.as_ref().ok_or("Browser not initialized")?;

        let idx = Self::parse_ref(ref_str)?;

        let text_escaped = text.replace('\\', "\\\\").replace('\'', "\\'");

        let js = format!(
            r#"(function() {{
                const selector = 'button, a, input, textarea, select, [tabindex], [role="button"], [role="link"], [role="textbox"], [role="combobox"], [role="checkbox"], [role="radio"], [contenteditable]';
                const els = document.querySelectorAll(selector);
                const el = els[{idx}];
                if (el) {{
                    el.focus();
                    if (el.value !== undefined) el.value = '{text}';
                    if (el.textContent !== undefined) el.textContent = '{text}';
                    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                    el.dispatchEvent(new Event('blur', {{ bubbles: true }}));
                    return 'typed';
                }}
                return 'element not found (index {idx})';
            }})()"#,
            idx = idx,
            text = text_escaped
        );

        page.evaluate_expression(&js)
            .await
            .map_err(|e| format!("Type into {} failed: {}", ref_str, e))?;

        Ok(ToolOutput::Text(format!("Typed {} characters into {}", text.len(), ref_str)))
    }

    async fn action_screenshot(&self) -> Result<ToolOutput, String> {
        self.ensure_browser().await?;
        let page_guard = self.page.lock().await;
        let page = page_guard.as_ref().ok_or("Browser not initialized")?;

        let bytes = page
            .screenshot(ScreenshotParams::default())
            .await
            .map_err(|e| format!("Screenshot failed: {}", e))?;

        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Ok(ToolOutput::Text(b64))
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        "Control a headless browser: navigate, snapshot, click, type, and screenshot"
    }

    fn schema(&self) -> ToolDef {
        ToolDef {
            name: "browser".into(),
            description: "Control a headless browser: navigate, snapshot, click, type, and screenshot".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["navigate", "snapshot", "click", "type", "screenshot"],
                        "description": "The browser action to perform"
                    },
                    "url": {
                        "type": "string",
                        "description": "URL to navigate to (required for navigate action)"
                    },
                    "ref": {
                        "type": "string",
                        "description": "Element reference (e.g. @e5) for click and type actions"
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to type into the element (required for type action)"
                    }
                },
                "required": ["action"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, String> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter: action".to_string())?;

        match action {
            "navigate" => {
                let url = params
                    .get("url")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing required parameter: url for navigate action".to_string())?;
                self.action_navigate(url).await
            }
            "snapshot" => self.action_snapshot().await,
            "click" => {
                let ref_str = params.get("ref").and_then(|v| v.as_str()).ok_or_else(|| {
                    "Missing required parameter: ref for click action".to_string()
                })?;
                self.action_click(ref_str).await
            }
            "type" => {
                let ref_str = params.get("ref").and_then(|v| v.as_str()).ok_or_else(|| {
                    "Missing required parameter: ref for type action".to_string()
                })?;
                let text = params.get("text").and_then(|v| v.as_str()).ok_or_else(|| {
                    "Missing required parameter: text for type action".to_string()
                })?;
                self.action_type(ref_str, text).await
            }
            "screenshot" => self.action_screenshot().await,
            _ => Err(format!(
                "Unknown action: {}. Must be one of: navigate, snapshot, click, type, screenshot",
                action
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn chromium_available() -> bool {
        let path = std::env::var("CHROME_PATH").unwrap_or_else(|_| "/usr/bin/chromium".to_string());
        std::path::Path::new(&path).exists()
    }

    #[tokio::test]
    async fn test_browser_navigate_and_snapshot() {
        if !chromium_available() {
            eprintln!("Skipping browser test: CHROME_PATH not available");
            return;
        }

        let tool = BrowserTool::new();
        let params = json!({"action": "navigate", "url": "data:text/html,<html><body><h1>Hello</h1><a href='#' id='link1'>Click me</a></body></html>"});
        let result = tool.execute(params).await.unwrap();
        match result {
            ToolOutput::Text(text) => {
                assert!(text.contains("@e1"), "Expected @e1 ref in snapshot: {}", text);
                assert!(text.contains("Click me"), "Expected 'Click me' in snapshot: {}", text);
            }
            ToolOutput::Error(e) => panic!("Expected success, got error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_browser_invalid_ref() {
        if !chromium_available() {
            return;
        }

        let tool = BrowserTool::new();
        let params = json!({"action": "click", "ref": "@x1"});
        let result = tool.execute(params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid element ref"));
    }

    #[tokio::test]
    async fn test_browser_missing_action() {
        let tool = BrowserTool::new();
        let params = json!({});
        let result = tool.execute(params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("action"));
    }

    #[tokio::test]
    async fn test_browser_unknown_action() {
        let tool = BrowserTool::new();
        let params = json!({"action": "fly"});
        let result = tool.execute(params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown action"));
    }
}
