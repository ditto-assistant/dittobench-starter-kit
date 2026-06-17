package scorer

import (
	"encoding/json"
	"testing"

	"github.com/ditto-assistant/dittobench-starter-kit/pkg/protocol"
)

func call(name string) protocol.ObservedToolCall {
	return protocol.ObservedToolCall{Name: name, Args: json.RawMessage(`{}`)}
}

func TestScoreCase_PerfectMatch(t *testing.T) {
	c := protocol.ToolCase{
		ID:            "c1",
		ExpectedTools: []protocol.ToolSpec{{Name: "search_web"}},
	}
	resp := protocol.RunResponse{ToolCalls: []protocol.ObservedToolCall{call("search_web")}}
	got := ScoreCase(c, resp)
	if got.ToolScore != 1.0 {
		t.Fatalf("want 1.0, got %v", got.ToolScore)
	}
}

func TestScoreCase_MissingTool(t *testing.T) {
	c := protocol.ToolCase{
		ID:            "c1",
		ExpectedTools: []protocol.ToolSpec{{Name: "search_web"}, {Name: "read_links"}},
	}
	resp := protocol.RunResponse{ToolCalls: []protocol.ObservedToolCall{call("search_web")}}
	got := ScoreCase(c, resp)
	if got.ToolScore != 0.5 {
		t.Fatalf("want 0.5, got %v", got.ToolScore)
	}
}

func TestScoreCase_ExtraPenalty(t *testing.T) {
	c := protocol.ToolCase{
		ID:              "c1",
		ExpectedTools:   []protocol.ToolSpec{{Name: "search_web"}},
		AllowExtraTools: false,
	}
	resp := protocol.RunResponse{ToolCalls: []protocol.ObservedToolCall{call("search_web"), call("create_image")}}
	got := ScoreCase(c, resp)
	// base 1.0 - 0.1 extra = 0.9
	if got.ToolScore < 0.89 || got.ToolScore > 0.91 {
		t.Fatalf("want ~0.9, got %v", got.ToolScore)
	}
}

func TestScoreCase_AllowExtra(t *testing.T) {
	c := protocol.ToolCase{
		ID:              "c1",
		ExpectedTools:   []protocol.ToolSpec{{Name: "search_web"}},
		AllowExtraTools: true,
	}
	resp := protocol.RunResponse{ToolCalls: []protocol.ObservedToolCall{call("search_web"), call("create_image")}}
	got := ScoreCase(c, resp)
	if got.ToolScore != 1.0 {
		t.Fatalf("want 1.0 (extras allowed), got %v", got.ToolScore)
	}
}

func TestScoreCase_AbstentionCorrect(t *testing.T) {
	c := protocol.ToolCase{ID: "c1", ExpectedTools: nil}
	resp := protocol.RunResponse{}
	got := ScoreCase(c, resp)
	if got.ToolScore != 1.0 {
		t.Fatalf("want 1.0 (correct abstention), got %v", got.ToolScore)
	}
}

func TestScoreCase_AbstentionViolated(t *testing.T) {
	c := protocol.ToolCase{ID: "c1", ExpectedTools: nil}
	resp := protocol.RunResponse{ToolCalls: []protocol.ObservedToolCall{call("search_web")}}
	got := ScoreCase(c, resp)
	if got.ToolScore != 0.0 {
		t.Fatalf("want 0.0 (should have abstained), got %v", got.ToolScore)
	}
}

func TestScore_CompositeAndMedian(t *testing.T) {
	cases := []protocol.ToolCase{
		{ID: "a", ExpectedTools: []protocol.ToolSpec{{Name: "search_web"}}},
		{ID: "b", ExpectedTools: []protocol.ToolSpec{{Name: "read_links"}}},
	}
	resps := map[string]protocol.RunResponse{
		"a": {ToolCalls: []protocol.ObservedToolCall{call("search_web")}, LatencyMs: 100},
		"b": {LatencyMs: 300}, // missed
	}
	rep := Score("run1", cases, resps)
	if rep.N != 2 {
		t.Fatalf("want N=2, got %d", rep.N)
	}
	if rep.Composite != 0.5 {
		t.Fatalf("want composite 0.5, got %v", rep.Composite)
	}
	if rep.MedianMs != 200 {
		t.Fatalf("want median 200, got %d", rep.MedianMs)
	}
}
