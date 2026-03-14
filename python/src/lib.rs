use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use wikipedia_article_transform::{strip_references, ArticleFormat, WikiPage};

fn build_items(
    html: &str,
    language: Option<&str>,
    include_references: bool,
) -> PyResult<Vec<wikipedia_article_transform::ArticleItem>> {
    let mut page = WikiPage::new().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    if let Some(lang) = language {
        if !lang.is_empty() {
            page.set_base_url(lang);
        }
    }
    let items = page
        .extract_text(html)
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    Ok(if include_references {
        items
    } else {
        strip_references(items)
    })
}

#[pyfunction(signature = (html, language=None))]
fn extract_plain(html: &str, language: Option<&str>) -> PyResult<String> {
    let items = build_items(html, language, true)?;
    Ok(items.format_plain())
}

#[pyfunction(signature = (html, language=None, include_references=true))]
fn extract_markdown(
    html: &str,
    language: Option<&str>,
    include_references: bool,
) -> PyResult<String> {
    let items = build_items(html, language, include_references)?;
    Ok(items.format_markdown())
}

#[pyfunction(signature = (html, language=None, include_references=true))]
fn extract_json(html: &str, language: Option<&str>, include_references: bool) -> PyResult<String> {
    let items = build_items(html, language, include_references)?;
    items
        .format_json()
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

#[pyfunction(signature = (html, format="plain", language=None, include_references=true))]
fn extract(
    html: &str,
    format: &str,
    language: Option<&str>,
    include_references: bool,
) -> PyResult<String> {
    match format {
        "plain" => {
            let items = build_items(html, language, include_references)?;
            Ok(items.format_plain())
        }
        "json" => extract_json(html, language, include_references),
        "markdown" => extract_markdown(html, language, include_references),
        _ => Err(PyValueError::new_err(
            "Invalid format. Use one of: plain, json, markdown",
        )),
    }
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(extract_plain, m)?)?;
    m.add_function(wrap_pyfunction!(extract_markdown, m)?)?;
    m.add_function(wrap_pyfunction!(extract_json, m)?)?;
    m.add_function(wrap_pyfunction!(extract, m)?)?;
    Ok(())
}
