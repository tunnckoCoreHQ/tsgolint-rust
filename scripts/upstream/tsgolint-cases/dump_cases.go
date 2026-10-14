// Added to oxc-project/tsgolint's internal/rule_tester by
// scripts/upstream/dump-cases.sh.
package rule_tester

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/go-json-experiment/json"
	"github.com/go-json-experiment/json/jsontext"
	"github.com/typescript-eslint/tsgolint/internal/rule"
)

// Dumps the cases of a RunRuleTester call to $TSGOLINT_DUMP_CASES/<test>.json
// for tsgolint-rust's rule tests, instead of running them.

type dumpError struct {
	MessageId   string                      `json:"messageId"`
	Line        int                         `json:"line,omitzero"`
	Column      int                         `json:"column,omitzero"`
	EndLine     int                         `json:"endLine,omitzero"`
	EndColumn   int                         `json:"endColumn,omitzero"`
	Suggestions []InvalidTestCaseSuggestion `json:"suggestions"`
}

type dumpCase struct {
	Index    int               `json:"index"`
	Code     string            `json:"code"`
	Only     bool              `json:"only,omitzero"`
	Skip     bool              `json:"skip,omitzero"`
	FileName string            `json:"fileName,omitzero"`
	Options  any               `json:"options"`
	TSConfig string            `json:"tsconfig,omitzero"`
	Tsx      bool              `json:"tsx,omitzero"`
	Files    map[string]string `json:"files,omitempty"`
	Output   *[]string         `json:"output,omitzero"`
	Errors   *[]dumpError      `json:"errors,omitzero"`
}

type dumpSuite struct {
	Test     string     `json:"test"`
	Rule     string     `json:"rule"`
	TSConfig string     `json:"tsconfig"`
	Valid    []dumpCase `json:"valid"`
	Invalid  []dumpCase `json:"invalid"`
}

func (s InvalidTestCaseSuggestion) MarshalJSON() ([]byte, error) {
	return json.Marshal(struct {
		MessageId string `json:"messageId"`
		Output    string `json:"output"`
	}{s.MessageId, s.Output})
}

func dumpCases(dir string, tsconfigPath string, t *testing.T, r *rule.Rule, valid []ValidTestCase, invalid []InvalidTestCase) {
	suite := dumpSuite{Test: t.Name(), Rule: r.Name, TSConfig: tsconfigPath, Valid: []dumpCase{}, Invalid: []dumpCase{}}
	for i, c := range valid {
		suite.Valid = append(suite.Valid, dumpCase{Index: i, Code: c.Code, Only: c.Only, Skip: c.Skip, FileName: c.FileName, Options: c.Options, TSConfig: c.TSConfig, Tsx: c.Tsx, Files: c.Files})
	}
	for i, c := range invalid {
		output := c.Output
		if output == nil {
			output = []string{}
		}
		errors := []dumpError{}
		for _, e := range c.Errors {
			suggestions := e.Suggestions
			if suggestions == nil {
				suggestions = []InvalidTestCaseSuggestion{}
			}
			errors = append(errors, dumpError{e.MessageId, e.Line, e.Column, e.EndLine, e.EndColumn, suggestions})
		}
		suite.Invalid = append(suite.Invalid, dumpCase{Index: i, Code: c.Code, Only: c.Only, Skip: c.Skip, FileName: c.FileName, Options: c.Options, TSConfig: c.TSConfig, Tsx: c.Tsx, Files: c.Files, Output: &output, Errors: &errors})
	}
	data, err := json.Marshal(suite, jsontext.WithIndent("  "), jsontext.EscapeForHTML(false), json.Deterministic(true))
	if err != nil {
		t.Fatal(err)
	}
	name := strings.ReplaceAll(t.Name(), "/", "__") + ".json"
	if err := os.WriteFile(filepath.Join(dir, name), data, 0o644); err != nil {
		t.Fatal(err)
	}
}
