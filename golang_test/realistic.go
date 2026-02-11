package main

import (
	"fmt"
	"sync"
)

type User struct {
	ID   int
	Name string
}

type Store struct {
	mu    sync.RWMutex
	users map[int]*User
	total int
}

func NewStore() *Store {
	return &Store{
		users: map[int]*User{},
	}
}

func (s *Store) Add(u *User) {
	s.mu.Lock()
	s.users[u.ID] = u
	s.total++
	s.mu.Unlock()
}

func (s *Store) Get(id int) (*User, bool) {
	s.mu.RLock()
	u, ok := s.users[id]
	s.mu.RUnlock()
	return u, ok
}

func (s *Store) Snapshot() []User {
	s.mu.RLock()
	out := make([]User, 0, len(s.users))
	for _, u := range s.users {
		out = append(out, *u)
	}
	s.mu.RUnlock()
	return out
}

func UpdateNames(users []User, suffix string) {
	for i := range users {
		users[i].Name = users[i].Name + suffix
	}
}

func main() {
	store := NewStore()

	store.Add(&User{ID: 1, Name: "Ann"})
	store.Add(&User{ID: 2, Name: "Bob"})

	// Shadowing and := mixed
	x := 10
	if x := x + 1; x > 10 {
		fmt.Println("inner x", x)
	}
	fmt.Println("outer x", x)

	// Type switch with implicit variable
	var any interface{} = store
	switch v := any.(type) {
	case *Store:
		_ = v.total
	default:
		_ = v
	}

	// Goroutines and capture
	var wg sync.WaitGroup
	for i := 0; i < 3; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			u, ok := store.Get(i + 1)
			if ok {
				fmt.Println("user", u.Name)
			}
		}(i)
	}

	// Capture of outer variable
	count := 0
	func() {
		count++
	}()

	// Race-like pattern (no lock around total)
	go func() {
		store.total++
	}()

	wg.Wait()

	snap := store.Snapshot()
	UpdateNames(snap, "_ok")
	fmt.Println(len(snap))
}
