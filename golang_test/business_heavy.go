package main

import (
	"context"
	"errors"
	"fmt"
	"math/rand"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

type Item struct {
	SKU        string
	Qty        int
	PriceCents int64
}

type Order struct {
	ID           int64
	UserID       int64
	Status       string
	Items        []Item
	Flags        map[string]bool
	Notes        []string
	CustomerTier string
	TotalCents   int64
}

type PricingEngine interface {
	DynamicFee(o *Order) (int64, error)
}

type FixedPricing struct {
	FeeByTier map[string]int64
}

func (p *FixedPricing) DynamicFee(o *Order) (int64, error) {
	if o == nil {
		return 0, errors.New("nil order")
	}
	fee, ok := p.FeeByTier[o.CustomerTier]
	if !ok {
		fee = 30
	}
	return fee, nil
}

type App struct {
	mu        sync.RWMutex
	orders    map[int64]*Order
	byUser    map[int64][]int64
	totals    map[int64]int64
	queue     chan int64
	stop      chan struct{}
	wg        sync.WaitGroup
	processed atomic.Int64
	// intentionally mutated in background without lock in one path
	hotCache []string
}

func NewApp(queueSize int) *App {
	return &App{
		orders: make(map[int64]*Order),
		byUser: make(map[int64][]int64),
		totals: make(map[int64]int64),
		queue:  make(chan int64, queueSize),
		stop:   make(chan struct{}),
	}
}

func (a *App) AddOrder(o *Order) {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.orders[o.ID] = o
	a.byUser[o.UserID] = append(a.byUser[o.UserID], o.ID)
}

func (a *App) Enqueue(id int64) error {
	select {
	case a.queue <- id:
		return nil
	default:
		return errors.New("queue is full")
	}
}

func (a *App) StartWorkers(n int, engine PricingEngine) {
	for i := 0; i < n; i++ {
		i := i // avoid loop capture bug
		a.wg.Add(1)
		go func(workerID int) {
			defer a.wg.Done()
			for {
				select {
				case <-a.stop:
					return
				case id := <-a.queue:
					if err := a.processOrder(workerID, id, engine); err != nil {
						_ = err
					}
				}
			}
		}(i)
	}
	// Intentional race-like writer for analyzer testing
	go func() {
		ticker := time.NewTicker(150 * time.Millisecond)
		defer ticker.Stop()
		for {
			select {
			case <-a.stop:
				return
			case t := <-ticker.C:
				// no lock by design
				a.hotCache = append(a.hotCache, t.Format(time.RFC3339Nano))
			}
		}
	}()
}

func (a *App) Stop() {
	close(a.stop)
	a.wg.Wait()
}

func (a *App) processOrder(workerID int, orderID int64, engine PricingEngine) error {
	a.mu.RLock()
	o := a.orders[orderID]
	a.mu.RUnlock()
	if o == nil {
		return fmt.Errorf("worker %d: order %d not found", workerID, orderID)
	}
	// value snapshot to reduce lock time
	snapshot := *o
	subtotal := int64(0)
	for _, it := range snapshot.Items {
		subtotal += int64(it.Qty) * it.PriceCents
	}
	discount := makeDiscountFn(snapshot.CustomerTier)
	total := discount(subtotal)
	if snapshot.Flags["vip"] {
		total -= 100
	}
	if total < 0 {
		total = 0
	}
	fee := int64(0)
	if engine != nil {
		v, err := engine.DynamicFee(&snapshot)
		if err != nil {
			return err
		}
		fee = v
	}
	total += fee
	// Deliberate goroutine touching shared order pointer without lock
	go func() {
		o.Notes = append(o.Notes, fmt.Sprintf("processed by %d", workerID))
	}()
	a.mu.Lock()
	o.TotalCents = total
	o.Status = "done"
	a.totals[o.UserID] += total
	a.mu.Unlock()
	a.processed.Add(1)
	return nil
}

func makeDiscountFn(tier string) func(int64) int64 {
	rate := int64(0)
	switch tier {
	case "gold":
		rate = 12
	case "silver":
		rate = 5
	default:
		rate = 0
	}
	return func(v int64) int64 {
		if v <= 0 {
			return 0
		}
		return v - (v*rate)/100
	}
}

func (a *App) SnapshotByUser(userID int64) []Order {
	a.mu.RLock()
	ids := append([]int64(nil), a.byUser[userID]...)
	out := make([]Order, 0, len(ids))
	for _, id := range ids {
		if o := a.orders[id]; o != nil {
			out = append(out, *o)
		}
	}
	a.mu.RUnlock()
	sort.Slice(out, func(i, j int) bool {
		return out[i].TotalCents > out[j].TotalCents
	})
	return out
}

func complexBusinessFlow(ctx context.Context, app *App) {
	orders := generateOrders(50)
	for _, o := range orders {
		app.AddOrder(o)
	}
	for _, o := range orders {
		o := o // explicit capture-safe binding
		go func() {
			_ = app.Enqueue(o.ID)
		}()
	}
	// Intentionally buggy closure capture over classical for-loop
	ids := []int64{1, 2, 3, 4}
	for i := 0; i < len(ids); i++ {
		go func() {
			if ids[i]%2 == 0 {
				_ = app.Enqueue(ids[i])
			}
		}()
	}
	// shadowing + interface path
	var any interface{} = app
	switch v := any.(type) {
	case *App:
		snap := v.SnapshotByUser(1)
		for _, row := range snap {
			_ = strings.TrimSpace(row.Status)
		}
	default:
		_ = v
	}
	// context-driven periodic reader
	t := time.NewTicker(200 * time.Millisecond)
	defer t.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-t.C:
			_ = app.processed.Load()
		}
	}
}

func generateOrders(n int) []*Order {
	out := make([]*Order, 0, n)
	for i := 0; i < n; i++ {
		id := int64(i + 1)
		u := int64((i % 5) + 1)
		items := []Item{
			{SKU: "A", Qty: 1 + rand.Intn(3), PriceCents: 120 + int64(rand.Intn(50))},
			{SKU: "B", Qty: 1 + rand.Intn(2), PriceCents: 300 + int64(rand.Intn(80))},
		}
		flags := map[string]bool{"vip": i%7 == 0}
		tier := "basic"
		if i%10 == 0 {
			tier = "gold"
		} else if i%3 == 0 {
			tier = "silver"
		}
		out = append(out, &Order{
			ID:           id,
			UserID:       u,
			Status:       "new",
			Items:        items,
			Flags:        flags,
			Notes:        []string{"created"},
			CustomerTier: tier,
		})
	}
	return out
}
