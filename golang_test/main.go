package main

import (
	"fmt"
	"sync"
	"time"
)

var globalMu sync.Mutex
var globalCounter = 1

func init() {
	globalCounter = 42
}

type Node struct {
	Value int
	Next  *Node
}

type WorkerPool struct {
	mu      sync.Mutex
	tasks   []int
	results map[int]int
	total   int
}

func (p *WorkerPool) addTask(v int) {
	p.mu.Lock()
	p.tasks = append(p.tasks, v)
	p.mu.Unlock()
}

func (p *WorkerPool) sumResults() int {
	sum := 0
	for _, v := range p.results {
		sum += v
	}
	return sum
}

func makeNode(v int) *Node {
	n := Node{Value: v}
	return &n
}

func compute(a int) (result int, err error) {
	if a < 0 {
		err = fmt.Errorf("neg")
		return
	}
	result = a * 2
	return
}

func main() {
	pool := &WorkerPool{
		tasks:   []int{1, 2, 3},
		results: map[int]int{},
	}
	pool.addTask(4)

	x := 1
	if x := x + 1; x > 1 {
		fmt.Println("shadow", x)
	}
	fmt.Println("outer", x)

	sum := 0
	for i, v := range pool.tasks {
		sum += v + i
	}
	fmt.Println("sum", sum)

	for i, v := range pool.tasks {
		_ = i
		if v%2 == 0 {
			continue
		}
		pool.results[v] = v * 10
	}

	total := 0
	func() {
		total++
	}()

	for i := 0; i < 2; i++ {
		go func() {
			globalMu.Lock()
			globalCounter += i
			globalMu.Unlock()
		}()
	}

	for i := 0; i < 2; i++ {
		i := i
		go func() {
			fmt.Println("fixed", i)
		}()
	}

	n := Node{Value: 7}
	p := &n
	p.Value += 1

	any := interface{}(pool)
	switch v := any.(type) {
	case *WorkerPool:
		fmt.Println("ts", v.total)
	default:
		fmt.Println("ts", v)
	}

	node := makeNode(9)
	fmt.Println(node.Value)

	r, err := compute(5)
	if err == nil {
		fmt.Println("compute", r, total)
	}

	time.Sleep(10 * time.Millisecond)
	fmt.Println(pool.sumResults())
}
