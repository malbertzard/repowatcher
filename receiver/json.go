package receiver

import (
	"encoding/json"
	"fmt"
	"log"
)

// JSONReceiver is a receiver that outputs results in JSON format.
type JSONReceiver struct {
	Messages []string `json:"messages"`
}

// NewJSONReceiver creates a new JSONReceiver instance.
func NewJSONReceiver() *JSONReceiver {
	return &JSONReceiver{}
}

// AddLine adds a line to the JSONReceiver.
func (r *JSONReceiver) AddLine(line string) {
	r.Messages = append(r.Messages, line)
}

// Output prints the JSONReceiver output.
func (r *JSONReceiver) Output() {
	data, err := json.MarshalIndent(r, "", "  ")
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println(string(data))
}
