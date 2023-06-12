package cmd

import "github.com/spf13/cobra"

var addCmd = &cobra.Command{
	Use:   "add",
	Short: "Add a repo",
	Run:   func(cmd *cobra.Command, args []string) {},
}

func GetAddCmd() *cobra.Command {
	return addCmd
}
