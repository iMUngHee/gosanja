package flow

import "testing"

func TestFlow(t *testing.T) {
	inbox := make(chan int, 1)
	inbox <- 5

	for _, sample := range []struct {
		name string
		n    int
		arg  any
		in   <-chan int
		want int
	}{
		{"negative and waiting message", -3, 7, inbox, 33},
		{"negative and other type", -1, nil, nil, 42},
		{"zero and string", 0, "hey", nil, 40},
		{"one and other type", 1, nil, nil, 48},
		{"two and negative argument", 2, -1, nil, 40},
	} {
		t.Run(sample.name, func(t *testing.T) {
			if got := Flow(sample.n, sample.arg, sample.in); got != sample.want {
				t.Fatalf("Flow = %d, want %d", got, sample.want)
			}
		})
	}
}

func TestInitializers(t *testing.T) {
	got := [5]int{A, B, F(), G(), Marker}
	want := [5]int{2, 3, 5, 8, 9}
	if got != want {
		t.Fatalf("initializers = %v, want %v", got, want)
	}
	if got := Tagged(); got != 21 {
		t.Errorf("Tagged = %d, want 21", got)
	}
	if got := Generated(); got != 34 {
		t.Errorf("Generated = %d, want 34", got)
	}
}
