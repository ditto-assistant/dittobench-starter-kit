// Package scorer scores a harness run against a DittoBench dataset. The logic
// mirrors the on-chain validator's tool-accuracy scoring so miners can measure
// progress offline before submitting.
package scorer

import (
	"fmt"
	"sort"
	"time"

	"github.com/ditto-assistant/dittobench-starter-kit/pkg/protocol"
)

// extraPenalty is subtracted per unexpected extra tool call when the case does
// not allow extra tools.
const extraPenalty = 0.1

// ScoreCase scores a single case against the harness response.
//
// Algorithm (mirrors Ditto's validator):
//   - For each expected tool, matched = min(expected_count, observed_count).
//   - base = sum(matched) / total_expected_count.
//   - If !AllowExtraTools, subtract extraPenalty per unexpected extra tool call.
//   - Clamp to [0, 1].
//   - A case with no expected tools scores 1.0 only if the harness called no
//     tools (correct abstention); otherwise 0.0.
func ScoreCase(c protocol.ToolCase, resp protocol.RunResponse) protocol.CaseScore {
	cs := protocol.CaseScore{
		CaseID:    c.ID,
		Category:  c.Category,
		LatencyMs: resp.LatencyMs,
	}

	// Observed tool-call counts by name.
	observed := map[string]int{}
	for _, tc := range resp.ToolCalls {
		observed[tc.Name]++
		cs.Called = append(cs.Called, tc.Name)
	}

	// Expected tool counts by name.
	expectedCounts := map[string]int{}
	for _, et := range c.ExpectedTools {
		expectedCounts[et.Name]++
		cs.Expected = append(cs.Expected, et.Name)
	}

	totalExpected := len(c.ExpectedTools)

	// Abstention case: no expected tools.
	if totalExpected == 0 {
		if len(resp.ToolCalls) == 0 {
			cs.ToolScore = 1.0
		} else {
			cs.ToolScore = 0.0
			cs.Notes = append(cs.Notes, fmt.Sprintf("expected abstention but called %d tool(s)", len(resp.ToolCalls)))
		}
		return cs
	}

	// Matched expected calls.
	matched := 0
	usedObserved := map[string]int{}
	for name, want := range expectedCounts {
		have := observed[name]
		m := have
		if want < m {
			m = want
		}
		matched += m
		usedObserved[name] = m
		if have < want {
			cs.Notes = append(cs.Notes, fmt.Sprintf("missing %d call(s) to %q", want-have, name))
		}
	}

	base := float64(matched) / float64(totalExpected)

	// Penalize unexpected extra calls when extras are not allowed.
	if !c.AllowExtraTools {
		extras := 0
		for name, have := range observed {
			// Calls beyond what was matched as expected are extras.
			beyond := have - usedObserved[name]
			if beyond > 0 {
				extras += beyond
			}
		}
		if extras > 0 {
			base -= extraPenalty * float64(extras)
			cs.Notes = append(cs.Notes, fmt.Sprintf("%d unexpected extra tool call(s)", extras))
		}
	}

	cs.ToolScore = clamp(base)
	return cs
}

// Score scores a full run: maps caseID -> RunResponse.
func Score(runID string, cases []protocol.ToolCase, resps map[string]protocol.RunResponse) protocol.ScoreReport {
	report := protocol.ScoreReport{
		RunID:       runID,
		GeneratedAt: time.Now().UTC().Format(time.RFC3339),
		N:           len(cases),
	}

	var sum float64
	var latencies []int64
	for _, c := range cases {
		resp := resps[c.ID]
		cs := ScoreCase(c, resp)
		report.PerCase = append(report.PerCase, cs)
		sum += cs.ToolScore
		latencies = append(latencies, cs.LatencyMs)
	}

	if len(cases) > 0 {
		report.ToolMean = sum / float64(len(cases))
	}
	report.Composite = report.ToolMean
	report.MedianMs = median(latencies)
	return report
}

func clamp(v float64) float64 {
	if v < 0 {
		return 0
	}
	if v > 1 {
		return 1
	}
	return v
}

func median(xs []int64) int64 {
	if len(xs) == 0 {
		return 0
	}
	cp := append([]int64(nil), xs...)
	sort.Slice(cp, func(i, j int) bool { return cp[i] < cp[j] })
	n := len(cp)
	if n%2 == 1 {
		return cp[n/2]
	}
	return (cp[n/2-1] + cp[n/2]) / 2
}
