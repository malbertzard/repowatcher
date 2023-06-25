package receiver

// Receiver defines the interface for receiving command output.
type Receiver interface {
	AddLine(line string)
	Output()
}
