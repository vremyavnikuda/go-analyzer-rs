package main

func main() {
	x := 42
	println(x)
	ptr := &x
	println(*ptr)
	go func() {
		println(x)
	}()
}
