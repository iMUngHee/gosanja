// Command gosanja-adapter-go is the Go adapter of gosanja: it answers `describe` and `index`
// as spec/adapter-contract.md (version 0.5) defines them.
package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"

	adaptergo "gosanja/adapters/go"
	"gosanja/adapters/go/internal/base"
	buildlayer "gosanja/adapters/go/internal/build"
	"gosanja/adapters/go/internal/contract"
	"gosanja/adapters/go/internal/env"
	"gosanja/adapters/go/internal/paths"
	"gosanja/adapters/go/internal/syntax"
)

const usage = "usage: gosanja-adapter-go describe | index --root <dir> --out <dir> [--layers <names>]"

func main() {
	caller, err := env.FromProcess()
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	os.Exit(run(os.Args[1:], caller, runtime.GOOS, os.Stdout, os.Stderr))
}

// Manifest is what `describe` prints.
func Manifest() contract.Manifest {
	return contract.Manifest{
		Contract:         contract.Version,
		Name:             "go",
		Version:          adaptergo.Version(),
		Languages:        []string{"go"},
		PositionEncoding: "utf-8",
		Layers:           []string{contract.Base, contract.Syntax, contract.Build},
		Requires:         []contract.Requirement{{Tool: "go"}, {Tool: "scip-go"}},
		Markers:          []string{"go.mod"},
	}
}

// invalid is an invocation the adapter refuses without writing anything.
type invalid struct{ reason string }

func (e invalid) Error() string { return e.reason }

func run(args []string, caller env.Caller, goos string, stdout, stderr io.Writer) int {
	switch {
	case len(args) == 1 && args[0] == "describe":
		if err := json.NewEncoder(stdout).Encode(Manifest()); err != nil {
			fmt.Fprintln(stderr, err)
			return 1
		}
		return 0
	case len(args) >= 1 && args[0] == "index":
		err := index(args[1:], caller, goos, stderr)
		var inv invalid
		switch {
		case err == nil:
			return 0
		case errors.As(err, &inv):
			fmt.Fprintln(stderr, inv.reason)
			fmt.Fprintln(stderr, usage)
			return 2
		default:
			fmt.Fprintln(stderr, err)
			return 1
		}
	}
	fmt.Fprintln(stderr, usage)
	return 2
}

type options struct {
	root, out string
	request   contract.Request
}

func parse(args []string) (options, error) {
	var o options
	values := map[string]string{}
	for i := 0; i < len(args); i += 2 {
		name := args[i]
		if name != "--root" && name != "--out" && name != "--layers" {
			return o, invalid{fmt.Sprintf("unknown option %q", name)}
		}
		if _, twice := values[name]; twice {
			return o, invalid{fmt.Sprintf("%s given twice", name)}
		}
		if i+1 >= len(args) {
			return o, invalid{fmt.Sprintf("%s needs a value", name)}
		}
		values[name] = args[i+1]
	}
	o.root, o.out = values["--root"], values["--out"]
	o.request = contract.Request{Syntax: true, Build: true}
	if o.root == "" || o.out == "" {
		return o, invalid{"--root and --out are required"}
	}
	if list, ok := values["--layers"]; ok {
		request, err := contract.ParseRequest(list, Manifest().Layers)
		if err != nil {
			return o, invalid{err.Error()}
		}
		o.request = request
	}
	return o, nil
}

func index(args []string, caller env.Caller, goos string, stderr io.Writer) error {
	o, err := parse(args)
	if err != nil {
		return err
	}
	root, out, err := locations(o, caller.Cwd)
	if err != nil {
		return err
	}
	outcome, err := base.Run(context.Background(), base.Config{Root: root, Out: out, GOOS: goos, Caller: caller, Stderr: stderr})
	if err != nil {
		return err
	}
	result := contract.Result{
		Contract: contract.Version,
		Layers:   map[string]contract.Entry{contract.Base: outcome.Entry},
		Tools:    outcome.Tools,
	}
	if o.request.Syntax {
		entry, err := syntaxLayer(root, out, outcome, stderr)
		if err != nil {
			return err
		}
		result.Layers[contract.Syntax] = entry
	}
	if o.request.Build {
		entry := contract.Failed("base-failed")
		if !outcome.Failed() {
			var rows bytes.Buffer
			entry, err = buildlayer.Layer(&rows, root, outcome)
			if err != nil {
				return err
			}
			if entry.Status == "ok" {
				if err := os.WriteFile(filepath.Join(out, "build.jsonl"), rows.Bytes(), 0o644); err != nil {
					return err
				}
			}
		}
		result.Layers[contract.Build] = entry
	}
	data, err := json.Marshal(result)
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(out, "result.json"), append(data, '\n'), 0o644)
}

// layer computes the syntax layer; tests replace it to see whether it is called.
var layer = syntax.Layer

// syntaxLayer runs the syntax layer over the base layer's outcome and writes syntax.jsonl when
// the layer succeeds. A failed base layer fails it with base-failed, without loading anything.
func syntaxLayer(root, out string, outcome base.Outcome, stderr io.Writer) (contract.Entry, error) {
	if outcome.Failed() {
		return contract.Failed("base-failed"), nil
	}
	files := make(map[string]syntax.File, len(outcome.Reads))
	for rel, read := range outcome.Reads {
		files[rel] = syntax.File{
			Code: read.Code, HasLineDirective: read.LineDirective,
		}
	}
	entry, facts := layer(context.Background(), syntax.Input{
		Root: root, Env: outcome.Env, Files: files, Documents: outcome.Documents,
		Inventory: outcome.Inventory.Owned(root), Stderr: stderr, Index: outcome.Index,
	})
	if entry.Status != "ok" {
		return entry, nil
	}
	var rows []byte
	for _, fact := range facts {
		row, err := json.Marshal(fact)
		if err != nil {
			return contract.Entry{}, err
		}
		rows = append(append(rows, row...), '\n')
	}
	return entry, os.WriteFile(filepath.Join(out, "syntax.jsonl"), rows, 0o644)
}

// locations checks --root and --out and resolves both: the root an existing directory, the
// output an existing empty directory that is not the root and does not lie beneath it, judged
// by file identity.
func locations(o options, cwd string) (root, out string, err error) {
	root, err = filepath.EvalSymlinks(abs(o.root, cwd))
	if err != nil {
		return "", "", invalid{fmt.Sprintf("--root %s: %v", o.root, err)}
	}
	rootInfo, err := os.Stat(root)
	if err != nil || !rootInfo.IsDir() {
		return "", "", invalid{fmt.Sprintf("--root %s is not a directory", o.root)}
	}
	out, err = filepath.EvalSymlinks(abs(o.out, cwd))
	if err != nil {
		return "", "", invalid{fmt.Sprintf("--out %s: %v", o.out, err)}
	}
	if info, err := os.Stat(out); err != nil || !info.IsDir() {
		return "", "", invalid{fmt.Sprintf("--out %s is not a directory", o.out)}
	}
	if entries, err := os.ReadDir(out); err != nil || len(entries) > 0 {
		return "", "", invalid{fmt.Sprintf("--out %s is not an empty directory", o.out)}
	}
	if inside, err := paths.Within(rootInfo, out); err != nil || inside {
		return "", "", invalid{fmt.Sprintf("--out %s lies in the root %s", o.out, o.root)}
	}
	return root, out, nil
}

func abs(path, cwd string) string {
	if filepath.IsAbs(path) {
		return path
	}
	return filepath.Join(cwd, path)
}
