package main

import (
	"fmt"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

// CommentNoiseState is a playground for comment-heavy code paths.
type CommentNoiseState struct {
	mu sync.RWMutex // protects guarded + store
	// plainCounter is intentionally touched from goroutine without lock in one place.
	plainCounter int64
	guarded      int64
	atomicCount  int64
	// Retention candidates
	window []byte
	label  string
	store  map[string][]byte
	wg sync.WaitGroup
}

func newCommentNoiseState() *CommentNoiseState {
	return &CommentNoiseState{
		store: map[string][]byte{},
	}
}

func (s *CommentNoiseState) setGuarded(v int64) {
	/*
		Even with block comments around lock/unlock,
		real lock calls below are the only ones that must matter.
	*/
	s.mu.Lock() // real lock
	s.guarded = v
	s.mu.Unlock() // real unlock
}

func (s *CommentNoiseState) getGuarded() int64 {
	s.mu.RLock() // read lock
	defer s.mu.RUnlock()
	return s.guarded
}

func (s *CommentNoiseState) mixedAtomicPath() {
	atomic.AddInt64(&s.atomicCount, 1) // atomic access
	// atomic.AddInt64(&s.atomicCount, 1) // comment only: must not count
	s.atomicCount++ // non-atomic access: mixed atomic/non-atomic
}

func (s *CommentNoiseState) retentionPath(input []byte) {
	// sub-slice can keep big backing array
	s.window = input[:16]
	big := strings.Repeat("A", 1<<19)
	/*
		sub-string can keep big backing buffer too
	*/
	s.label = big[:32]
	ext := map[string][]byte{"x": input}
	// map reference assignment
	s.store = ext
}

func (s *CommentNoiseState) commentDenseWorker(id int) {
	s.wg.Add(1)
	go func(workerID int) {
		defer s.wg.Done()
		// fake synchronization in comments only:
		// s.mu.Lock()
		// s.mu.Unlock()

		/*
			Fake write in comment:
			s.plainCounter++
		*/
		s.plainCounter += int64(workerID) // real unsynchronized write
		// Real synchronized write for guarded field.
		s.mu.Lock()
		s.guarded += int64(workerID)
		s.mu.Unlock()
	}(id)
}

func (s *CommentNoiseState) captureAfterUnlock() {
	s.mu.RLock()
	if len(s.store) == 0 {
		s.mu.RUnlock()
		return
	}
	s.mu.RUnlock()
	go func() {
		time.Sleep(time.Millisecond)
		_ = len(s.store) // access after unlock in goroutine
		_ = s.guarded    // also read outside lock in goroutine

		// go func(){ s.guarded++ }() // comment only
	}()
}

func (s *CommentNoiseState) multilineParams(
	prefix string, // inline param comment
	// another comment line between params
	suffix string,
) string {
	return prefix + suffix
}

func runCommentsLayoutCheck() {
	s := newCommentNoiseState()
	input := make([]byte, 0, 1<<20)
	input = append(input, []byte("hello-comment-noise")...)
	s.retentionPath(input)
	s.setGuarded(10)
	_ = s.getGuarded()
	s.mixedAtomicPath()
	for i := 0; i < 3; i++ {
		// weird inline comments around calls
		s.commentDenseWorker(i) // worker launch
	}
	s.captureAfterUnlock()
	_ = s.multilineParams(
		"left-", // first arg
		"right", // second arg
	)
	s.wg.Wait()
	fmt.Println("done", s.plainCounter)
}
