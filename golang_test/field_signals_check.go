package main

import (
	"fmt"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

type LargeEvent struct {
	ID      int64
	Name    string
	Payload [2048]byte
	Meta    [64]string
	Flags   [64]int64
}

type FieldSignalState struct {
	mu          sync.RWMutex
	counter     int64
	processed   int64
	balance     int64
	statusCode  int
	initialized bool
	hotWindow   []byte
	shortLabel  string
	sharedIndex map[string][]byte
	snapshot []LargeEvent
}

func consumeLarge(e LargeEvent) int64 {
	return e.ID
}

func (s *FieldSignalState) incWithoutLock() {
	s.counter++ // field race candidate
}

func (s *FieldSignalState) incAtomic() {
	atomic.AddInt64(&s.processed, 1)
}

func (s *FieldSignalState) incPlain() {
	s.processed++ // mixed atomic + non-atomic
}

func (s *FieldSignalState) setBalance(v int64) {
	s.mu.Lock()
	s.balance = v
	s.mu.Unlock()
}

func (s *FieldSignalState) unsafeBalanceRead() int64 {
	return s.balance // lock coverage violation candidate
}

func (s *FieldSignalState) updateStatus(code int) {
	s.statusCode = code // write-only candidate
}

func (s *FieldSignalState) checkInitialized() bool {
	if s.initialized { // read-before-write candidate
		return true
	}
	return false
}

func (s *FieldSignalState) initConfig() {
	s.initialized = true
}

func (s *FieldSignalState) loadRetention(raw []byte, external map[string][]byte) {
	big := strings.Repeat("x", 1<<20)
	s.hotWindow = raw[:8]    // retention: sub-slice
	s.shortLabel = big[:16]  // retention: sub-string
	s.sharedIndex = external // retention: map reference assignment
	s.sharedIndex["preview"] = raw[:4]
}

func (s *FieldSignalState) captureAfterUnlock() {
	s.mu.RLock()
	if len(s.snapshot) == 0 {
		s.mu.RUnlock()
		return
	}
	s.mu.RUnlock()
	go func() {
		time.Sleep(2 * time.Millisecond)
		_ = len(s.snapshot) // captured field access in goroutine
		_ = s.balance       // unsynchronized read in goroutine
	}()
}

func (s *FieldSignalState) heavyUnderLock(event LargeEvent) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.snapshot = append(s.snapshot, event)
	sort.Slice(s.snapshot, func(i, j int) bool {
		return s.snapshot[i].ID < s.snapshot[j].ID
	})
	_ = fmt.Sprintf("events=%d", len(s.snapshot))
}

func (s *FieldSignalState) copyByValue(event LargeEvent) int64 {
	localCopy := event
	return consumeLarge(localCopy) // large struct copy candidate
}

func runFieldSignalsCheck(raw []byte, external map[string][]byte, events []LargeEvent) int64 {
	state := &FieldSignalState{}
	_ = state.checkInitialized()
	state.initConfig()
	state.updateStatus(100)
	state.updateStatus(200)
	state.loadRetention(raw, external)
	state.setBalance(10)
	_ = state.unsafeBalanceRead()
	if len(events) > 0 {
		state.heavyUnderLock(events[0])
	}
	for i := 0; i < 3; i++ {
		go state.incWithoutLock()
		go state.incAtomic()
		go state.incPlain()
	}
	state.captureAfterUnlock()
	if len(events) > 0 {
		byValue := events[0]
		_ = consumeLarge(byValue)
		_ = state.copyByValue(byValue)
	}
	return state.counter + state.processed
}
