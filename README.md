# Gola

**Prompt-defined AI Agents** - Build, share, and run AI agents with YAML configuration and prompts.

## What is Gola?

Gola lets you create powerful AI agents without writing code. Define capabilities in YAML and prompts, connect to 5000+ tools via MCP, and share agents directly through GitHub.

## Quick Start

```bash
# Download latest release from https://github.com/gola-agent/gola/releases
# Example for Linux:
curl -L https://github.com/gola-agent/gola/releases/latest/download/gola-linux-amd64 -o gola
chmod +x gola

# Create agent config
cat > gola.yaml << EOF
agent:
  name: "My Assistant"
llm:
  provider: openai
  model: "gpt-4o-mini"
  auth:
    api_key_env: "OPENAI_API_KEY"
EOF

# Run
export OPENAI_API_KEY="your-key"
./gola --config gola.yaml
```

## Run Agents from GitHub

```bash
# Run any agent directly from GitHub
gola --config github:username/my-agent

# With specific version
gola --config github:username/my-agent@v1.0.0
```

## Key Features

- **Zero-code agent creation** - YAML configuration and prompts
- **GitHub-native distribution** - Fork, improve, and share agents
- **MCP integration** - Connect to thousands of tools and APIs
- **Multiple LLM providers** - OpenAI, Anthropic, Google Gemini
- **RAG support** - Embed documents for knowledge-based responses
- **Interactive terminal** - Built-in chat interface

## Documentation

Full documentation, examples, and guides available at **[gola.chat](https://gola.chat)**

## License

MIT