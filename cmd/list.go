package cmd

import (
	"fmt"

	"github.com/spf13/cobra"
	"github.com/spf13/viper"
)

var listCmd = &cobra.Command{
	Use:   "list",
	Short: "Displays a list of all the nicknames and paths to the directories stored in the config",
	Run: func(cmd *cobra.Command, args []string) {
		directories := viper.GetStringMap("directories")

		if len(directories) == 0 {
			fmt.Println("No directories found in config")
			return
		}

		fmt.Println("Nickname\tPath")
		fmt.Println("========\t====")
		for nickname, path := range directories {
			fmt.Printf("%s\t%s\n", nickname, path)
		}
	},
}

func GetListCmd() *cobra.Command {
	return listCmd
}
