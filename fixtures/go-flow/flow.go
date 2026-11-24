package flow

import (
	"example.com/goflow/meter"
	"golang.org/x/mod/semver"
)

// Count is the fixture's public alias for an integer.
type Count = int

const DefaultCount Count = 2

var A, B = seed(2), next(2)
var F, G = func() int { return seed(5) }, func() int { return seed(8) }
var _, Marker = seed(1), seed(9)

// Shape describes a value that can be scored.
type Shape interface {
	Score() int
}

// Loader describes the method that only *meter.Gauge provides.
type Loader interface {
	Load() int
}

var _ Shape = meter.Circle{}
var _ Loader = (*meter.Gauge)(nil)

type EmbeddedValue struct {
	meter.Circle
}

type EmbeddedPointer struct {
	*meter.Gauge
}

//go:noinline
func seed(n int) int { return n }

func Max[T ~int](a, b T) T { return meter.Max(a, b) }

func make2() func(int) int {
	return func(n int) int { return seed(n) }
}

func IsVersion(version string) bool { return semver.IsValid(version) }

// Promoted calls one method through an embedded value and one through an embedded pointer.
func Promoted() int {
	circle := meter.Circle{Amount: 2}
	value := EmbeddedValue{Circle: circle}
	pointer := EmbeddedPointer{Gauge: &meter.Gauge{Amount: 3}}
	return circle.Score() + value.Score() + pointer.Load()
}

func signal(done chan<- struct{})      { close(done) }
func awaitSignal(done <-chan struct{}) { <-done }

// Flow collects the statement forms used in the outline fixture.
func Flow(n int, argument any, inbox <-chan int) int {
	score := int(meter.Double(int32(n)))
	fs := []func() int{func() int { return seed(4) }}
	values := append([]int{score}, Max[int](1, 2), meter.Max[int](2, 3), meter.Max(3, 4), make2()(1), fs[0]())
	개수 := len(values)
	total := 개수 + seed(values[0])
	done := make(chan struct{})
	go signal(done)
	defer awaitSignal(done)

	if n < 0 {
		n = -n
		goto classify
	} else if n == 0 {
		total += 1
	} else {
		total += 2
	}

classify:
	switch n % 3 {
	case 0:
		total += seed(3)
		fallthrough
	case 1:
		total += seed(5)
	default:
		total += seed(7)
	}

	switch value := argument.(type) {
	case int:
		total += value
	case string:
		total += len(value)
	default:
		total += seed(11)
	}

	steps := []int{1, 2, 3, 4}
	for i := 0; i < len(steps); i++ {
		if i == 1 {
			continue
		}
		if i == 3 && inbox == nil {
			break
		}
		total += steps[i]
	}
	for _, delta := range []int{2, 3} {
		total += delta
	}

	select {
	case value := <-inbox:
		total += value
	default:
		total += seed(13)
	}

	bump := func(value int) int { return seed(value) }
	return bump(total)
}

func next(n int) int { return n + 1 }

var _ = seed(1)
