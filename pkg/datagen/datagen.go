// Package datagen procedurally generates DittoBench tool-calling datasets from
// templates using a seeded PRNG. The same seed yields a deterministic dataset;
// different seeds yield different prompts. This lets miners practice the
// run+score loop offline against fresh-looking cases.
//
// NOTE: the on-chain validator uses its own (held-out) generator. This is a
// practice generator only — do not overfit to it.
package datagen

import (
	"fmt"
	"math/rand"
	"time"

	"github.com/ditto-assistant/dittobench-starter-kit/pkg/protocol"
)

// category describes one generation template.
type category struct {
	name     string
	tools    []string // expected tool names (empty = abstention/no-tool)
	maxCalls int
	build    func(r *rand.Rand) (prompt string, behavior string)
}

// Word pools for randomized entities/phrasings.
var (
	topics    = []string{"quantum computing", "the French Revolution", "GraphQL", "sourdough starters", "Rust ownership", "the Voyager probes", "tariffs", "mitochondria", "the Bittensor subnet design", "Kubernetes operators"}
	people    = []string{"Ada", "Grace", "Linus", "Margaret", "Dennis", "Barbara", "Alan", "Katherine"}
	subjects  = []string{"my startup", "my dissertation", "the Q3 budget", "my marathon training", "the kitchen remodel", "my reading list", "the family reunion", "my side project"}
	sites     = []string{"https://example.com/post", "https://news.example.org/article", "https://blog.example.dev/entry", "https://docs.example.io/guide"}
	imageIdea = []string{"a neon city skyline at dusk", "a watercolor fox in a forest", "an isometric cozy bedroom", "a retro sci-fi book cover", "a minimalist mountain logo"}
	artifacts = []string{"README.md", "notes.txt", "plan.md", "todo.md", "config.yaml"}
	jobs      = []string{"refactor the auth module", "add unit tests for the parser", "migrate the config to TOML", "fix the flaky integration test", "scaffold a REST endpoint"}
	chitchat  = []string{"How are you doing today?", "Tell me a fun fact.", "What's a good name for a cat?", "Explain recursion simply.", "Give me a motivational quote."}
)

func pick(r *rand.Rand, pool []string) string { return pool[r.Intn(len(pool))] }

var categories = []category{
	{
		name: "memory_lookup", tools: []string{"search_memories"}, maxCalls: 2,
		build: func(r *rand.Rand) (string, string) {
			return fmt.Sprintf("What did I say earlier about %s?", pick(r, subjects)),
				"Should call search_memories to recall prior user statements."
		},
	},
	{
		name: "memory_subject", tools: []string{"search_subjects"}, maxCalls: 2,
		build: func(r *rand.Rand) (string, string) {
			return fmt.Sprintf("Who is %s in my notes?", pick(r, people)),
				"Should call search_subjects to find the entity."
		},
	},
	{
		name: "web_search", tools: []string{"search_web"}, maxCalls: 2,
		build: func(r *rand.Rand) (string, string) {
			return fmt.Sprintf("What's the latest news on %s?", pick(r, topics)),
				"Should call search_web for fresh information."
		},
	},
	{
		name: "link_read", tools: []string{"read_links"}, maxCalls: 2,
		build: func(r *rand.Rand) (string, string) {
			return fmt.Sprintf("Summarize this page for me: %s", pick(r, sites)),
				"Should call read_links on the provided URL."
		},
	},
	{
		name: "image_create", tools: []string{"create_image"}, maxCalls: 2,
		build: func(r *rand.Rand) (string, string) {
			return fmt.Sprintf("Make me an image of %s.", pick(r, imageIdea)),
				"Should call create_image with a prompt."
		},
	},
	{
		name: "artifacts_create", tools: []string{"artifacts"}, maxCalls: 2,
		build: func(r *rand.Rand) (string, string) {
			return fmt.Sprintf("Create a file called %s with my project outline.", pick(r, artifacts)),
				"Should call artifacts with action=create."
		},
	},
	{
		name: "agent_job", tools: []string{"execute_agent_job"}, maxCalls: 2,
		build: func(r *rand.Rand) (string, string) {
			return fmt.Sprintf("Kick off an agent to %s.", pick(r, jobs)),
				"Should call execute_agent_job with the task."
		},
	},
	{
		name: "settings_theme", tools: []string{"set_theme"}, maxCalls: 2,
		build: func(r *rand.Rand) (string, string) {
			mode := pick(r, []string{"dark", "light"})
			return fmt.Sprintf("Switch my app to %s mode.", mode),
				"Should call set_theme."
		},
	},
	{
		name: "settings_effort", tools: []string{"set_reasoning_effort"}, maxCalls: 2,
		build: func(r *rand.Rand) (string, string) {
			lvl := pick(r, []string{"high", "low", "medium"})
			return fmt.Sprintf("Set reasoning effort to %s please.", lvl),
				"Should call set_reasoning_effort."
		},
	},
	{
		name: "no_tool", tools: nil, maxCalls: 0,
		build: func(r *rand.Rand) (string, string) {
			return pick(r, chitchat),
				"Pure conversation; the harness should NOT call any tool."
		},
	},
	{
		name: "abstention", tools: nil, maxCalls: 0,
		build: func(r *rand.Rand) (string, string) {
			return fmt.Sprintf("Just chatting — no need to look anything up. By the way, %s",
				pick(r, chitchat)),
				"The harness should abstain from tool use."
		},
	},
}

// Generate builds a deterministic dataset of n cases for the given seed.
func Generate(seed int64, n int) protocol.Dataset {
	r := rand.New(rand.NewSource(seed))
	ds := protocol.Dataset{
		Seed:        seed,
		GeneratedAt: time.Now().UTC().Format(time.RFC3339),
	}
	for i := 0; i < n; i++ {
		cat := categories[r.Intn(len(categories))]
		prompt, behavior := cat.build(r)
		c := protocol.ToolCase{
			ID:               fmt.Sprintf("%s-%04d", cat.name, i),
			Category:         cat.name,
			Prompt:           prompt,
			MaxToolCalls:     cat.maxCalls,
			AllowExtraTools:  false,
			ExpectedBehavior: behavior,
		}
		for _, tn := range cat.tools {
			c.ExpectedTools = append(c.ExpectedTools, protocol.ToolSpec{Name: tn})
		}
		ds.ToolCases = append(ds.ToolCases, c)
	}
	return ds
}
