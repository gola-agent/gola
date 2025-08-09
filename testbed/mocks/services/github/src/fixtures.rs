use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub name: String,
    pub owner: String,
    pub description: Option<String>,
    pub default_branch: String,
    pub private: bool,
    pub branches: Vec<String>,
    pub tags: Vec<String>,
    pub files: HashMap<String, FileContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContent {
    pub content: String,
    pub encoding: String, // "utf-8" or "base64"
}

#[derive(Debug, Clone)]
pub struct RepositoryFixture {
    repositories: HashMap<String, Repository>,
}

impl RepositoryFixture {
    pub fn new() -> Self {
        Self {
            repositories: HashMap::new(),
        }
    }

    pub fn add_repository(&mut self, key: String, repo: Repository) {
        self.repositories.insert(key, repo);
    }

    pub fn get_repository(&self, owner: &str, name: &str) -> Option<&Repository> {
        let key = format!("{}/{}", owner, name);
        self.repositories.get(&key)
    }

    pub fn from_yaml(yaml_content: &str) -> anyhow::Result<Self> {
        let repos: HashMap<String, Repository> = serde_yaml::from_str(yaml_content)?;
        Ok(Self {
            repositories: repos,
        })
    }

    pub fn create_test_fixture() -> Self {
        let mut fixture = Self::new();
        
        // Create a test repository with gola.yaml
        let mut files = HashMap::new();
        files.insert(
            "gola.yaml".to_string(),
            FileContent {
                content: r#"agent:
  name: "Test Agent"

llm:
  provider: openai
  model: "gpt-4o-mini"
  auth:
    api_key_env: "OPENAI_API_KEY"
"#.to_string(),
                encoding: "utf-8".to_string(),
            },
        );
        files.insert(
            "README.md".to_string(),
            FileContent {
                content: "# Test Repository\n\nThis is a test repository for GitHub mock.".to_string(),
                encoding: "utf-8".to_string(),
            },
        );

        let repo = Repository {
            name: "test-repo".to_string(),
            owner: "testuser".to_string(),
            description: Some("A test repository for GitHub mock".to_string()),
            default_branch: "main".to_string(),
            private: false,
            branches: vec!["main".to_string(), "develop".to_string()],
            tags: vec!["v1.0.0".to_string(), "v1.1.0".to_string()],
            files,
        };

        fixture.add_repository("testuser/test-repo".to_string(), repo);
        
        // Also create testuser/gola-config repository for integration tests
        let mut gola_config_files = HashMap::new();
        gola_config_files.insert(
            "gola.yaml".to_string(),
            FileContent {
                content: r#"agent:
  name: "GitHub Mock Test Agent"
  description: "Agent configuration for testing GitHub mock integration"

llm:
  provider: openai
  model: "gpt-4o-mini"
  auth:
    api_key_env: "OPENAI_API_KEY"

tools:
  - name: echo
    description: "Echo back the input"
"#.to_string(),
                encoding: "utf-8".to_string(),
            },
        );
        
        let gola_config_repo = Repository {
            name: "gola-config".to_string(),
            owner: "testuser".to_string(),
            description: Some("Test gola configuration repository".to_string()),
            default_branch: "main".to_string(),
            private: false,
            branches: vec!["main".to_string()],
            tags: vec!["v1.0.0".to_string()],
            files: gola_config_files,
        };

        fixture.add_repository("testuser/gola-config".to_string(), gola_config_repo);
        fixture
    }
}