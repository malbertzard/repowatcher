package cmd

import (
	"github.com/spf13/cobra"
)

var jumpCmd = &cobra.Command{
	Use:   "jump [nickname]",
	Short: "Changes the current working directory to the directory associated with the passed nickname",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
	},
}

func GetJumpCmd() *cobra.Command {
	return jumpCmd
}
