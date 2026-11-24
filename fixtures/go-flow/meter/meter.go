package meter

type Circle struct {
	Amount int
}

func (v Circle) Score() int { return v.Amount }

type Gauge struct {
	Amount int
}

func (g *Gauge) Load() int { return g.Amount }

type Box[T any] struct {
	item T
}

func NewBox[T any](item T) Box[T] { return Box[T]{item: item} }

func (b Box[T]) Get() T { return b.item }

func Double(n int32) int32 { return n * 2 }

func Max[T ~int](a, b T) T {
	if a > b {
		return a
	}
	return b
}
