package receiver

import "fmt"

// TextReceiver is a receiver that outputs results in plain text format.
type TextReceiver struct {
	lines []string
}

// NewTextReceiver creates a new TextReceiver instance.
func NewTextReceiver() *TextReceiver {
	return &TextReceiver{}
}

// AddLine adds a line to the TextReceiver.
func (r *TextReceiver) AddLine(line string) {
	r.lines = append(r.lines, line)
}

// Output prints the TextReceiver output.
func (r *TextReceiver) Output() {
	for _, line := range r.lines {
		fmt.Println(line)
	}
}
