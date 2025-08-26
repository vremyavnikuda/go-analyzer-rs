package main

import (
	"fmt"
	"os"
)

func main() {
	x := 42                    // 🟢 Declaration
	fmt.Fprintln(os.Stdout, x) // 🟡 Usage
	x = 100                    // 🟣 Reassignment
	ptr := &x                  // 🟡 Usage of x, 🟢 Declaration of ptr
	println(*ptr)              // 🔵 Pointer usage
	go func() {
		println(x) // 🟪 Captured variable in goroutine
	}()
}
