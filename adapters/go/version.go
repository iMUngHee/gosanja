// Package adaptergo holds what the adapter's packages share at the module root: its version.
package adaptergo

import (
	_ "embed"
	"strings"
)

//go:embed VERSION
var version string

// Version is the adapter's own version, the content of VERSION without its final newline.
func Version() string { return strings.TrimSuffix(version, "\n") }
