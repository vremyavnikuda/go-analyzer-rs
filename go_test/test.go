package main

import (
	"fmt"
	"math/rand"
	"sort"
	"sync"
	"time"
)

type Task struct {
	ID    int
	Value int
}

type WorkerPool struct {
	Tasks       []Task
	Result      map[int]int
	Mutex       sync.Mutex
	SharedTotal int // <- нарочно без мьютекса (data race)
	WG          sync.WaitGroup
}

func main() {
	rand.Seed(time.Now().UnixNano())

	pool := &WorkerPool{
		Tasks:  generateTasks(100),
		Result: make(map[int]int),
	}

	// Стартуем 5 воркеров
	for i := 0; i < 5; i++ {
		pool.WG.Add(1)
		go pool.worker(i)
	}

	// Сортировка в отдельной горутине с блокировкой
	go func() {
		for {
			time.Sleep(2 * time.Second)
			pool.Mutex.Lock()
			taskSnapshot := append([]Task(nil), pool.Tasks...)
			pool.Mutex.Unlock()

			sort.Slice(taskSnapshot, func(i, j int) bool {
				return taskSnapshot[i].Value < taskSnapshot[j].Value
			})

			fmt.Println("[INFO] Top 3 tasks:", taskSnapshot[:3])
		}
	}()

	pool.WG.Wait()

	fmt.Println("\n🔧 Финальные результаты:")
	for k, v := range pool.Result {
		fmt.Printf("Task %d -> %d\n", k, v)
	}
	fmt.Println("Shared Total (возможна ошибка из-за гонки):", pool.SharedTotal)
}

func (p *WorkerPool) worker(id int) {
	defer p.WG.Done()

	for {
		p.Mutex.Lock()
		if len(p.Tasks) == 0 {
			p.Mutex.Unlock()
			fmt.Printf("Worker #%d завершил работу\n", id)
			return
		}
		task := p.Tasks[0]
		p.Tasks = p.Tasks[1:]
		p.Mutex.Unlock()

		// сложное выражение с блоками и задержкой
		result := complexComputation(task.Value)

		// намеренный data race (SharedTotal без мьютекса)
		p.SharedTotal += result

		// безопасная запись
		p.Mutex.Lock()
		p.Result[task.ID] = result
		p.Mutex.Unlock()

		fmt.Printf("Worker #%d обработал Task %d = %d\n", id, task.ID, result)
	}
}

func complexComputation(x int) int {
	// имитируем тяжёлую работу
	time.Sleep(time.Duration(rand.Intn(200)) * time.Millisecond)

	// сложное выражение: рекурсия, ветвления, случайности
	if x%7 == 0 {
		return x * x
	} else if x%5 == 0 {
		return fibonacci(x % 10)
	}
	return x + rand.Intn(50)
}

func fibonacci(n int) int {
	if n < 2 {
		return 1
	}
	return fibonacci(n-1) + fibonacci(n-2)
}

func generateTasks(n int) []Task {
	tasks := make([]Task, n)
	for i := range tasks {
		tasks[i] = Task{
			ID:    i + 1,
			Value: rand.Intn(100),
		}
	}
	return tasks
}
