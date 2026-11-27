use std::panic::{AssertUnwindSafe, catch_unwind};

pub fn pdf_text(bytes: &[u8]) -> Result<String, String> {
    match catch_unwind(AssertUnwindSafe(|| pdf_extract::extract_text_from_mem(bytes))) {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err("the PDF could not be read".into()),
    }
}

#[cfg(test)]
mod tests;
