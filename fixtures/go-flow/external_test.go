package flow_test

import (
	"testing"

	"example.com/goflow"
	"example.com/goflow/meter"
)

func TestPublicCalls(t *testing.T) {
	if got := flow.Max(3, 9); got != 9 {
		t.Errorf("Max = %d, want 9", got)
	}
	if got := flow.Max(9, 3); got != 9 {
		t.Errorf("Max = %d, want 9", got)
	}
	if !flow.IsVersion("v1.2.3") || flow.IsVersion("1.2.3") {
		t.Error("IsVersion accepted a version without the v prefix or rejected a valid version")
	}
	if got := flow.Promoted(); got != 7 {
		t.Errorf("Promoted = %d, want 7", got)
	}
	if got := meter.NewBox("token").Get(); got != "token" {
		t.Errorf("Box.Get = %q, want token", got)
	}
}
