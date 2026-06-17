// Package mocktools provides a default harness.ToolExecutor that returns
// plausible mock results per tool. Offline practice does not need real web /
// memory / side effects — the goal is to exercise the tool-calling loop and
// score which tools the model chose, not to perform the action.
package mocktools

import (
	"encoding/json"
	"fmt"

	"github.com/ditto-assistant/dittobench-starter-kit/pkg/harness"
)

// New returns a ToolExecutor that produces canned, plausible results so the
// harness loop can progress to a final answer.
func New() harness.ToolExecutor {
	return func(name string, args json.RawMessage) (string, error) {
		switch name {
		case "search_web":
			return `[{"title":"Result A","url":"https://example.com/a","snippet":"Relevant info about the query."},{"title":"Result B","url":"https://example.com/b","snippet":"More context."}]`, nil
		case "read_links":
			return `{"content":"(mock) The page discusses the requested topic in detail.","words":842}`, nil
		case "search_memories":
			return `[{"id":"mem_001","text":"(mock) Earlier the user mentioned a relevant detail.","score":0.91}]`, nil
		case "search_subjects":
			return `[{"subject":"(mock) Matched subject","memories":3}]`, nil
		case "search_memories_in_subjects":
			return `[{"id":"mem_042","text":"(mock) Memory scoped to subject.","score":0.88}]`, nil
		case "fetch_memories":
			return `{"outline":["mem_001: (mock) note","mem_002: (mock) note"]}`, nil
		case "create_image":
			return `{"image_url":"https://cdn.example.com/generated/mock.png","status":"ready"}`, nil
		case "edit_image":
			return `{"image_url":"https://cdn.example.com/edited/mock.png","status":"ready"}`, nil
		case "artifacts":
			return `{"status":"ok","artifact":"(mock) created/updated"}`, nil
		case "execute_agent_job":
			return `{"job_id":"job_mock_123","status":"queued"}`, nil
		case "execute_agent_workflow":
			return `{"run_id":"wf_mock_456","status":"started"}`, nil
		case "get_agent_job_status":
			return `{"job_id":"job_mock_123","status":"completed","summary":"(mock) done"}`, nil
		case "list_agent_jobs":
			return `[{"job_id":"job_mock_123","status":"completed"}]`, nil
		case "file_feedback_for_team":
			return `{"status":"received","ticket":"FB-mock-1"}`, nil
		case "set_theme", "set_main_model", "set_reasoning_effort",
			"set_chat_tool_preferences":
			return `{"status":"applied"}`, nil
		default:
			return fmt.Sprintf(`{"status":"ok","tool":%q,"note":"(mock) generic result"}`, name), nil
		}
	}
}
