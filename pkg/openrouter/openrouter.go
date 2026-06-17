// Package openrouter provides a reference harness.Model implementation backed
// by OpenRouter's /api/v1/chat/completions endpoint with tool calling. It is
// dependency-free (net/http + encoding/json) and reads OPENROUTER_API_KEY from
// the environment.
//
// Miners can swap this out for any Model implementation; it's provided as a
// working baseline so `practice` runs end to end with just an API key.
package openrouter

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"

	"github.com/ditto-assistant/dittobench-starter-kit/pkg/harness"
	"github.com/ditto-assistant/dittobench-starter-kit/pkg/protocol"
)

const defaultEndpoint = "https://openrouter.ai/api/v1/chat/completions"

// Client implements harness.Model against OpenRouter.
type Client struct {
	Model      string
	APIKey     string
	Endpoint   string
	HTTPClient *http.Client
}

// New returns a Client for the given model, reading OPENROUTER_API_KEY from env.
func New(model string) *Client {
	return &Client{
		Model:      model,
		APIKey:     os.Getenv("OPENROUTER_API_KEY"),
		Endpoint:   defaultEndpoint,
		HTTPClient: &http.Client{Timeout: 120 * time.Second},
	}
}

// ---- wire types for the OpenAI-compatible chat/completions API ----

type chatTool struct {
	Type     string       `json:"type"`
	Function functionSpec `json:"function"`
}

type functionSpec struct {
	Name        string          `json:"name"`
	Description string          `json:"description,omitempty"`
	Parameters  json.RawMessage `json:"parameters,omitempty"`
}

type chatMessage struct {
	Role       string         `json:"role"`
	Content    string         `json:"content,omitempty"`
	Name       string         `json:"name,omitempty"`
	ToolCallID string         `json:"tool_call_id,omitempty"`
	ToolCalls  []wireToolCall `json:"tool_calls,omitempty"`
}

type wireToolCall struct {
	ID       string `json:"id,omitempty"`
	Type     string `json:"type,omitempty"`
	Function struct {
		Name      string `json:"name"`
		Arguments string `json:"arguments"`
	} `json:"function"`
}

type chatRequest struct {
	Model    string        `json:"model"`
	Messages []chatMessage `json:"messages"`
	Tools    []chatTool    `json:"tools,omitempty"`
}

type chatResponse struct {
	Choices []struct {
		Message struct {
			Content   string         `json:"content"`
			ToolCalls []wireToolCall `json:"tool_calls"`
		} `json:"message"`
	} `json:"choices"`
	Usage struct {
		PromptTokens     int64 `json:"prompt_tokens"`
		CompletionTokens int64 `json:"completion_tokens"`
	} `json:"usage"`
	Error *struct {
		Message string `json:"message"`
	} `json:"error,omitempty"`
}

// Next implements harness.Model.
func (c *Client) Next(ctx context.Context, msgs []harness.Message, tools []protocol.ToolDefinition) (harness.Chunk, error) {
	if c.APIKey == "" {
		return harness.Chunk{}, fmt.Errorf("openrouter: OPENROUTER_API_KEY not set")
	}

	reqBody := chatRequest{Model: c.Model}
	for _, m := range msgs {
		cm := chatMessage{Role: m.Role, Content: m.Content, Name: m.Name, ToolCallID: m.ToolCallID}
		for i, tc := range m.ToolCalls {
			wtc := wireToolCall{ID: fmt.Sprintf("call_%d", i), Type: "function"}
			wtc.Function.Name = tc.Name
			if len(tc.Args) > 0 {
				wtc.Function.Arguments = string(tc.Args)
			} else {
				wtc.Function.Arguments = "{}"
			}
			cm.ToolCalls = append(cm.ToolCalls, wtc)
		}
		reqBody.Messages = append(reqBody.Messages, cm)
	}
	for _, t := range tools {
		reqBody.Tools = append(reqBody.Tools, chatTool{
			Type: "function",
			Function: functionSpec{
				Name:        t.Name,
				Description: t.Description,
				Parameters:  t.Parameters,
			},
		})
	}

	payload, err := json.Marshal(reqBody)
	if err != nil {
		return harness.Chunk{}, fmt.Errorf("openrouter: marshal request: %w", err)
	}

	endpoint := c.Endpoint
	if endpoint == "" {
		endpoint = defaultEndpoint
	}
	httpReq, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, bytes.NewReader(payload))
	if err != nil {
		return harness.Chunk{}, fmt.Errorf("openrouter: new request: %w", err)
	}
	httpReq.Header.Set("Content-Type", "application/json")
	httpReq.Header.Set("Authorization", "Bearer "+c.APIKey)
	httpReq.Header.Set("HTTP-Referer", "https://ditto.ai")
	httpReq.Header.Set("X-Title", "DittoBench Miner")

	hc := c.HTTPClient
	if hc == nil {
		hc = http.DefaultClient
	}
	httpResp, err := hc.Do(httpReq)
	if err != nil {
		return harness.Chunk{}, fmt.Errorf("openrouter: do request: %w", err)
	}
	defer httpResp.Body.Close()

	body, err := io.ReadAll(httpResp.Body)
	if err != nil {
		return harness.Chunk{}, fmt.Errorf("openrouter: read body: %w", err)
	}
	if httpResp.StatusCode != http.StatusOK {
		return harness.Chunk{}, fmt.Errorf("openrouter: status %d: %s", httpResp.StatusCode, string(body))
	}

	var cr chatResponse
	if err := json.Unmarshal(body, &cr); err != nil {
		return harness.Chunk{}, fmt.Errorf("openrouter: unmarshal response: %w", err)
	}
	if cr.Error != nil {
		return harness.Chunk{}, fmt.Errorf("openrouter: api error: %s", cr.Error.Message)
	}
	if len(cr.Choices) == 0 {
		return harness.Chunk{}, fmt.Errorf("openrouter: no choices in response")
	}

	choice := cr.Choices[0]
	chunk := harness.Chunk{
		Text:         choice.Message.Content,
		PromptTokens: cr.Usage.PromptTokens,
		OutputTokens: cr.Usage.CompletionTokens,
	}
	for _, tc := range choice.Message.ToolCalls {
		args := json.RawMessage(tc.Function.Arguments)
		if len(args) == 0 {
			args = json.RawMessage(`{}`)
		}
		chunk.ToolCalls = append(chunk.ToolCalls, protocol.ObservedToolCall{
			Name: tc.Function.Name,
			Args: args,
		})
	}
	return chunk, nil
}
