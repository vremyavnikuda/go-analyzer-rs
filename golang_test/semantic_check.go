package main

import "sync"

// semanticCheck содержит случаи для проверки:
// reassign, capture, shadowing, := mixed, range, pointer/reference types.
func semanticCheck() {
	// Shadowing
	a := 1
	if a := a + 1; a > 1 {
		_ = a
	}
	_ = a
	// Reassignments
	x := 1
	x = 2
	x += 3
	x++
	_ = x
	// Mixed := (частичное переобъявление)
	y := 10
	y, z := y+1, 5
	_ = y
	_ = z
	// Range reassign
	arr := []int{1, 2, 3}
	for i := range arr {
		i = i + 1
		_ = i
	}
	i2 := 0
	for i2 = range arr {
		_ = i2
	}
	// Capture in closure
	outer := 100
	f := func() int {
		outer++
		return outer
	}
	_ = f()
	// Capture in goroutine
	done := make(chan struct{})
	go func() {
		_ = outer
		close(done)
	}()
	<-done
	// Non-capture
	func() {
		inner := 1
		_ = inner
	}()
	// Pointer and reference types
	p := &outer
	s := []int{1, 2}
	m := map[string]int{"a": 1}
	ch := make(chan int, 1)
	fn := func(v int) int { return v + outer }
	var iface interface{} = s
	_ = p
	_ = m
	ch <- 1
	_ = <-ch
	_ = fn(1)
	_ = iface
	// Simple sync usage (for race-detection heuristics)
	var mu sync.Mutex
	mu.Lock()
	mu.Unlock()
	// Type switch (scope + shadowing)
	var any interface{} = m
	switch v := any.(type) {
	case map[string]int:
		_ = v["a"]
	default:
		_ = v
	}
}
