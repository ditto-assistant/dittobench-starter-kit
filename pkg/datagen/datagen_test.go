package datagen

import (
	"reflect"
	"testing"
)

func TestGenerate_DeterministicPerSeed(t *testing.T) {
	a := Generate(42, 30)
	b := Generate(42, 30)
	if !reflect.DeepEqual(a.ToolCases, b.ToolCases) {
		t.Fatal("same seed produced different cases")
	}
}

func TestGenerate_VarietyAcrossSeeds(t *testing.T) {
	a := Generate(1, 30)
	b := Generate(2, 30)
	if reflect.DeepEqual(a.ToolCases, b.ToolCases) {
		t.Fatal("different seeds produced identical cases")
	}
}

func TestGenerate_Count(t *testing.T) {
	ds := Generate(7, 25)
	if len(ds.ToolCases) != 25 {
		t.Fatalf("want 25 cases, got %d", len(ds.ToolCases))
	}
}

func TestGenerate_AbstentionHasNoExpectedTools(t *testing.T) {
	ds := Generate(99, 200)
	sawAbstain := false
	sawTool := false
	for _, c := range ds.ToolCases {
		if c.Category == "no_tool" || c.Category == "abstention" {
			sawAbstain = true
			if len(c.ExpectedTools) != 0 {
				t.Fatalf("abstention case %s has expected tools", c.ID)
			}
		} else {
			sawTool = true
			if len(c.ExpectedTools) == 0 {
				t.Fatalf("tool case %s has no expected tools", c.ID)
			}
		}
	}
	if !sawAbstain || !sawTool {
		t.Fatal("expected both abstention and tool cases across 200 samples")
	}
}
