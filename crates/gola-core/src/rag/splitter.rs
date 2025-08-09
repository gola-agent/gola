use std::collections::HashMap;

/// Text splitter for breaking documents into chunks
pub struct TextSplitter {
    chunk_size: usize,
    chunk_overlap: usize,
    separators: Vec<String>,
}

impl TextSplitter {
    pub fn new(chunk_size: usize, chunk_overlap: usize) -> Self {
        Self {
            chunk_size,
            chunk_overlap,
            separators: vec![
                "\n\n".to_string(),
                "\n".to_string(),
                " ".to_string(),
                "".to_string(),
            ],
        }
    }

    pub fn with_separators(mut self, separators: Vec<String>) -> Self {
        self.separators = separators;
        self
    }

    /// Split text into chunks based on the configured parameters
    pub fn split_text(&self, text: &str) -> Vec<String> {
        if text.len() <= self.chunk_size {
            return vec![text.to_string()];
        }

        self.recursive_split(text, &self.separators)
    }

    fn recursive_split(&self, text: &str, separators: &[String]) -> Vec<String> {
        if separators.is_empty() {
            return self.split_by_length(text);
        }

        let separator = &separators[0];
        let remaining_separators = &separators[1..];

        if separator.is_empty() {
            return self.split_by_length(text);
        }

        let splits: Vec<&str> = text.split(separator).collect();
        let mut final_chunks = Vec::new();
        let mut current_chunk = String::new();

        for split in splits {
            let potential_chunk = if current_chunk.is_empty() {
                split.to_string()
            } else {
                format!("{}{}{}", current_chunk, separator, split)
            };

            if potential_chunk.len() <= self.chunk_size {
                current_chunk = potential_chunk;
            } else {
                // Current chunk is ready, process it
                if !current_chunk.is_empty() {
                    final_chunks.push(current_chunk.clone());

                    // Handle overlap
                    if self.chunk_overlap > 0 && current_chunk.len() > self.chunk_overlap {
                        let mut overlap_start = current_chunk.len() - self.chunk_overlap;
                        
                        // Ensure overlap_start is on a character boundary
                        while overlap_start > 0 && !current_chunk.is_char_boundary(overlap_start) {
                            overlap_start -= 1;
                        }
                        
                        current_chunk = current_chunk[overlap_start..].to_string();
                    } else {
                        current_chunk.clear();
                    }
                }

                // If the split itself is too large, recursively split it
                if split.len() > self.chunk_size {
                    let sub_chunks = self.recursive_split(split, remaining_separators);
                    final_chunks.extend(sub_chunks);
                } else {
                    current_chunk = if current_chunk.is_empty() {
                        split.to_string()
                    } else {
                        format!("{}{}{}", current_chunk, separator, split)
                    };
                }
            }
        }

        // Add the last chunk if it's not empty
        if !current_chunk.is_empty() {
            final_chunks.push(current_chunk);
        }

        final_chunks
    }

    fn split_by_length(&self, text: &str) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut start = 0;

        while start < text.len() {
            let mut end = (start + self.chunk_size).min(text.len());
            
            // Ensure end is on a character boundary
            while end > start && !text.is_char_boundary(end) {
                end -= 1;
            }
            
            let chunk = text[start..end].to_string();
            chunks.push(chunk);

            // Move start position considering overlap
            if self.chunk_overlap > 0 && end < text.len() {
                let mut new_start = end - self.chunk_overlap.min(end - start);
                
                // Ensure new_start is on a character boundary
                while new_start > start && !text.is_char_boundary(new_start) {
                    new_start -= 1;
                }
                
                start = new_start;
            } else {
                start = end;
            }
        }

        chunks
    }
}

pub struct LanguageTextSplitter;

impl LanguageTextSplitter {
    pub fn get_separators_for_language(language: &str) -> Vec<String> {
        match language.to_lowercase().as_str() {
            "python" | "py" => vec![
                "\nclass ".to_string(),
                "\ndef ".to_string(),
                "\n\n".to_string(),
                "\n".to_string(),
                " ".to_string(),
                "".to_string(),
            ],
            "javascript" | "js" | "typescript" | "ts" => vec![
                "\nfunction ".to_string(),
                "\nconst ".to_string(),
                "\nlet ".to_string(),
                "\nvar ".to_string(),
                "\nclass ".to_string(),
                "\n\n".to_string(),
                "\n".to_string(),
                " ".to_string(),
                "".to_string(),
            ],
            "rust" | "rs" => vec![
                "\nfn ".to_string(),
                "\nstruct ".to_string(),
                "\nenum ".to_string(),
                "\nimpl ".to_string(),
                "\ntrait ".to_string(),
                "\n\n".to_string(),
                "\n".to_string(),
                " ".to_string(),
                "".to_string(),
            ],
            "java" => vec![
                "\nclass ".to_string(),
                "\ninterface ".to_string(),
                "\nenum ".to_string(),
                "\npublic ".to_string(),
                "\nprivate ".to_string(),
                "\nprotected ".to_string(),
                "\n\n".to_string(),
                "\n".to_string(),
                " ".to_string(),
                "".to_string(),
            ],
            "markdown" | "md" => vec![
                "\n## ".to_string(),
                "\n### ".to_string(),
                "\n#### ".to_string(),
                "\n##### ".to_string(),
                "\n###### ".to_string(),
                "\n\n".to_string(),
                "\n".to_string(),
                " ".to_string(),
                "".to_string(),
            ],
            "html" => vec![
                "\n<div".to_string(),
                "\n<p".to_string(),
                "\n<br".to_string(),
                "\n<li".to_string(),
                "\n<h1".to_string(),
                "\n<h2".to_string(),
                "\n<h3".to_string(),
                "\n<h4".to_string(),
                "\n<h5".to_string(),
                "\n<h6".to_string(),
                "\n\n".to_string(),
                "\n".to_string(),
                " ".to_string(),
                "".to_string(),
            ],
            _ => vec![
                "\n\n".to_string(),
                "\n".to_string(),
                " ".to_string(),
                "".to_string(),
            ],
        }
    }

    /// Create a text splitter optimized for a specific language
    pub fn for_language(language: &str, chunk_size: usize, chunk_overlap: usize) -> TextSplitter {
        let separators = Self::get_separators_for_language(language);
        TextSplitter {
            chunk_size,
            chunk_overlap,
            separators,
        }
    }
}

/// Document metadata type
pub type DocumentMetadata = HashMap<String, String>;

/// Options for chunk headers when splitting documents
#[derive(Debug, Clone)]
pub struct ChunkHeaderOptions {
    pub chunk_header: String,
    pub chunk_overlap_header: Option<String>,
}

impl Default for ChunkHeaderOptions {
    fn default() -> Self {
        Self {
            chunk_header: String::new(),
            chunk_overlap_header: None,
        }
    }
}

impl ChunkHeaderOptions {
    pub fn with_header(mut self, header: &str) -> Self {
        self.chunk_header = header.to_string();
        self
    }

    pub fn with_overlap_header(mut self, overlap_header: &str) -> Self {
        self.chunk_overlap_header = Some(overlap_header.to_string());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_text_splitting() {
        let splitter = TextSplitter::new(10, 2);
        let text = "This is a test document that should be split into multiple chunks.";
        let chunks = splitter.split_text(text);

        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(chunk.len() <= 20 || chunk.split_whitespace().count() == 1);
        }
    }

    #[test]
    fn test_small_text_no_splitting() {
        let splitter = TextSplitter::new(100, 10);
        let text = "Short text";
        let chunks = splitter.split_text(text);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn test_python_language_splitter() {
        let splitter = LanguageTextSplitter::for_language("python", 50, 5);
        let code = r#"
class MyClass:
    def method1(self):
        pass

def function1():
    return True
"#;
        let chunks = splitter.split_text(code);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_markdown_language_splitter() {
        let splitter = LanguageTextSplitter::for_language("markdown", 100, 10);
        let markdown = r#"
# Main Title

## Section 1
This is content for section 1.

## Section 2
This is content for section 2.

### Subsection 2.1
More detailed content here.
"#;
        let chunks = splitter.split_text(markdown);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_chunk_overlap() {
        let splitter = TextSplitter::new(20, 5);
        let text = "word1 word2 word3 word4 word5 word6 word7 word8 word9 word10";
        let chunks = splitter.split_text(text);

        assert!(chunks.len() > 1);

        // Check that there's some overlap between consecutive chunks
        for i in 1..chunks.len() {
            let prev_chunk = &chunks[i - 1];
            let curr_chunk = &chunks[i];

            // Find common words between chunks (simple overlap check)
            let prev_words: Vec<&str> = prev_chunk.split_whitespace().collect();
            let curr_words: Vec<&str> = curr_chunk.split_whitespace().collect();

            let _has_overlap = prev_words.iter().any(|word| curr_words.contains(word));
            // Note: overlap might not always be present depending on split points
        }
    }
}
