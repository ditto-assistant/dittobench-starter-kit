// Package catalog provides the canonical Ditto tool catalog as a slice of
// protocol.ToolDefinition. The DittoBench validator typically sends a subset
// of these tools per case; miners can use this catalog to build their own
// routing/prompting and to practice offline.
package catalog

import (
	"encoding/json"

	"github.com/ditto-assistant/dittobench-starter-kit/pkg/protocol"
)

// schema is a tiny helper for building a minimal JSON-schema object.
func schema(props map[string]any, required ...string) json.RawMessage {
	obj := map[string]any{
		"type":       "object",
		"properties": props,
	}
	if len(required) > 0 {
		obj["required"] = required
	}
	b, _ := json.Marshal(obj)
	return b
}

func str(desc string) map[string]any  { return map[string]any{"type": "string", "description": desc} }
func num(desc string) map[string]any  { return map[string]any{"type": "number", "description": desc} }
func boolp(desc string) map[string]any { return map[string]any{"type": "boolean", "description": desc} }
func enum(desc string, vals ...string) map[string]any {
	return map[string]any{"type": "string", "description": desc, "enum": vals}
}

// Catalog returns the full Ditto tool catalog.
func Catalog() []protocol.ToolDefinition {
	return []protocol.ToolDefinition{
		{
			Name:        "create_image",
			Description: "Generate a new image from a text prompt.",
			Parameters: schema(map[string]any{
				"prompt": str("Detailed description of the image to generate."),
				"style":  str("Optional style hint (e.g. photorealistic, anime)."),
			}, "prompt"),
		},
		{
			Name:        "edit_image",
			Description: "Edit or transform an existing image given an instruction.",
			Parameters: schema(map[string]any{
				"image_url":   str("URL or artifact reference of the source image."),
				"instruction": str("How to edit the image."),
			}, "image_url", "instruction"),
		},
		{
			Name:        "read_links",
			Description: "Fetch and read the contents of one or more web links/URLs.",
			Parameters: schema(map[string]any{
				"urls": map[string]any{
					"type":        "array",
					"items":       map[string]any{"type": "string"},
					"description": "List of URLs to read.",
				},
			}, "urls"),
		},
		{
			Name:        "search_web",
			Description: "Search the public web for fresh information.",
			Parameters: schema(map[string]any{
				"query": str("The web search query."),
			}, "query"),
		},
		{
			Name:        "search_memories",
			Description: "Semantic search across the user's long-term memories.",
			Parameters: schema(map[string]any{
				"query": str("What to search the user's memories for."),
				"limit": num("Maximum number of memories to return."),
			}, "query"),
		},
		{
			Name:        "search_subjects",
			Description: "Find subjects (entities/topics) in the user's memory graph.",
			Parameters: schema(map[string]any{
				"query": str("Subject or entity to look up."),
			}, "query"),
		},
		{
			Name:        "fetch_memories",
			Description: "Fetch specific memories by ID, or an outline of recent memories.",
			Parameters: schema(map[string]any{
				"ids":    map[string]any{"type": "array", "items": map[string]any{"type": "string"}, "description": "Memory IDs to fetch."},
				"format": enum("Output format.", "outline", "full"),
			}),
		},
		{
			Name:        "search_memories_in_subjects",
			Description: "Search memories scoped to one or more known subjects.",
			Parameters: schema(map[string]any{
				"query":    str("What to search for within the subjects."),
				"subjects": map[string]any{"type": "array", "items": map[string]any{"type": "string"}, "description": "Subject names to scope the search to."},
			}, "query", "subjects"),
		},
		{
			Name:        "artifacts",
			Description: "Create or update a persistent artifact (file/document) in the user's project.",
			Parameters: schema(map[string]any{
				"name":    str("Artifact file name/path."),
				"content": str("Artifact contents."),
				"action":  enum("Operation to perform.", "create", "update", "read"),
			}, "name", "action"),
		},
		{
			Name:        "execute_agent_job",
			Description: "Kick off an autonomous coding/agent job for a longer task.",
			Parameters: schema(map[string]any{
				"task":    str("Description of the job for the agent to perform."),
				"project": str("Optional project directory to operate in."),
			}, "task"),
		},
		{
			Name:        "execute_agent_workflow",
			Description: "Run a predefined multi-step agent workflow.",
			Parameters: schema(map[string]any{
				"workflow": str("Workflow name or identifier."),
				"inputs":   map[string]any{"type": "object", "description": "Workflow input parameters."},
			}, "workflow"),
		},
		{
			Name:        "get_agent_job_status",
			Description: "Check the status of a previously launched agent job.",
			Parameters: schema(map[string]any{
				"job_id": str("The agent job ID to check."),
			}, "job_id"),
		},
		{
			Name:        "list_agent_jobs",
			Description: "List the user's recent agent jobs and their statuses.",
			Parameters: schema(map[string]any{
				"limit": num("Maximum number of jobs to list."),
			}),
		},
		{
			Name:        "file_feedback_for_team",
			Description: "File a bug report or feature feedback to the Ditto team.",
			Parameters: schema(map[string]any{
				"category": enum("Type of feedback.", "bug", "feature", "other"),
				"message":  str("The feedback content."),
			}, "message"),
		},
		{
			Name:        "set_theme",
			Description: "Change the user's UI theme.",
			Parameters: schema(map[string]any{
				"theme": enum("Theme to apply.", "light", "dark", "system"),
			}, "theme"),
		},
		{
			Name:        "set_main_model",
			Description: "Set the user's default chat model.",
			Parameters: schema(map[string]any{
				"model": str("Model identifier to set as default."),
			}, "model"),
		},
		{
			Name:        "set_reasoning_effort",
			Description: "Set the reasoning effort level for the assistant.",
			Parameters: schema(map[string]any{
				"effort": enum("Reasoning effort level.", "low", "medium", "high"),
			}, "effort"),
		},
		{
			Name:        "set_chat_tool_preferences",
			Description: "Enable or disable specific tools in the user's chat preferences.",
			Parameters: schema(map[string]any{
				"enabled":  map[string]any{"type": "array", "items": map[string]any{"type": "string"}, "description": "Tools to enable."},
				"disabled": map[string]any{"type": "array", "items": map[string]any{"type": "string"}, "description": "Tools to disable."},
			}),
		},
	}
}

// ByName returns the tool definition with the given name, and whether it exists.
func ByName(name string) (protocol.ToolDefinition, bool) {
	for _, t := range Catalog() {
		if t.Name == name {
			return t, true
		}
	}
	return protocol.ToolDefinition{}, false
}
