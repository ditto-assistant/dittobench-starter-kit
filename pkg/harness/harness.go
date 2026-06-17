// Package harness is the slim, self-contained agent harness miners extend and
// optimize. It mirrors the canonical (private) ditto-harness shape:
//
//	Model.Next(ctx, []Message, []ToolDefinition) (Chunk, error)
//	Harness{Model, Exec, MaxHops}.Run(ctx, RunRequest) (RunResponse, error)
//
// The multi-turn loop here is the surface miners control: plug in your own
// Model (the openrouter.Client is a reference impl), your own ToolExecutor
// routing, and your own system prompt to move the DittoBench score.
package harness

import (
	"context"
	"encoding/json"
	"fmt"
	"time"

	"github.com/ditto-assistant/dittobench-starter-kit/pkg/protocol"
)

// Message is one turn in the chat transcript fed to the Model.
type Message struct {
	Role       string `json:"role"`                   // system | user | assistant | tool
	Content    string `json:"content"`                // text content
	Name       string `json:"name,omitempty"`         // tool name (for role=tool)
	ToolCallID string `json:"tool_call_id,omitempty"` // links a tool result to its call
	// ToolCalls carries assistant-issued tool calls so the model sees its own
	// prior calls in the transcript.
	ToolCalls []protocol.ObservedToolCall `json:"tool_calls,omitempty"`
}

// Chunk is one model response: text and/or tool calls, plus token usage.
type Chunk struct {
	Text         string
	ToolCalls    []protocol.ObservedToolCall
	PromptTokens int64
	OutputTokens int64
}

// Model is the LLM backend. Implement this to bring your own model/provider.
type Model interface {
	Next(ctx context.Context, msgs []Message, tools []protocol.ToolDefinition) (Chunk, error)
}

// ToolExecutor runs a tool call and returns a result string (the tool output
// fed back to the model). For offline practice this is a mock; on the validator
// the relevant tools are executed for real.
type ToolExecutor func(name string, args json.RawMessage) (string, error)

// Harness drives the multi-turn tool-calling loop.
type Harness struct {
	Model   Model
	Exec    ToolExecutor
	MaxHops int // safety cap on tool-calling rounds (default 6 if <= 0)
}

// Run executes one DittoBench case: it feeds the system+user prompt and tools
// to the Model, executes any tool calls via Exec, feeds results back, and
// repeats up to MaxHops. It returns the aggregated RunResponse.
func (h *Harness) Run(ctx context.Context, req protocol.RunRequest) (protocol.RunResponse, error) {
	if h.Model == nil {
		return protocol.RunResponse{}, fmt.Errorf("harness: nil Model")
	}
	maxHops := h.MaxHops
	if maxHops <= 0 {
		maxHops = 6
	}

	start := time.Now()
	msgs := make([]Message, 0, 4)
	if req.SystemPrompt != "" {
		msgs = append(msgs, Message{Role: "system", Content: req.SystemPrompt})
	}
	msgs = append(msgs, Message{Role: "user", Content: req.UserInput})

	var resp protocol.RunResponse

	for hop := 0; hop < maxHops; hop++ {
		chunk, err := h.Model.Next(ctx, msgs, req.Tools)
		if err != nil {
			resp.LatencyMs = time.Since(start).Milliseconds()
			return resp, fmt.Errorf("harness: model.Next (hop %d): %w", hop, err)
		}
		resp.PromptTokens += chunk.PromptTokens
		resp.OutputTokens += chunk.OutputTokens

		// Record the assistant turn (text and/or tool calls).
		assistant := Message{Role: "assistant", Content: chunk.Text, ToolCalls: chunk.ToolCalls}
		msgs = append(msgs, assistant)

		if len(chunk.ToolCalls) == 0 {
			// Final answer.
			resp.FinalText = chunk.Text
			resp.LatencyMs = time.Since(start).Milliseconds()
			return resp, nil
		}

		// Execute each tool call and feed results back.
		for i, tc := range chunk.ToolCalls {
			tc.Hop = hop
			resp.ToolCalls = append(resp.ToolCalls, tc)

			result := ""
			if h.Exec != nil {
				out, err := h.Exec(tc.Name, tc.Args)
				if err != nil {
					result = fmt.Sprintf("error: %v", err)
				} else {
					result = out
				}
			} else {
				result = "ok"
			}
			callID := tc.Name
			if id := toolCallID(hop, i); id != "" {
				callID = id
			}
			msgs = append(msgs, Message{
				Role:       "tool",
				Name:       tc.Name,
				ToolCallID: callID,
				Content:    result,
			})
		}
	}

	// Exhausted hops without a final text answer.
	resp.LatencyMs = time.Since(start).Milliseconds()
	return resp, nil
}

func toolCallID(hop, idx int) string {
	return fmt.Sprintf("call_%d_%d", hop, idx)
}
