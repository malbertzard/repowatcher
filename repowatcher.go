package main

import (
	"fmt"
	"os"

	"github.com/malbertzard/repowatcher/cmd"
	"github.com/spf13/cobra"
	"github.com/spf13/viper"
)

var cfgFile string

var rootCmd = &cobra.Command{
    Use:   "repowatcher",
    Short: "Watches your Git repositories for changes and performs various actions",
}

func init() {
    cobra.OnInitialize(initConfig)

    rootCmd.PersistentFlags().StringVar(&cfgFile, "config", "", "config file (default is $HOME/.config/repowatcher/config.yaml)")

    //TODO Init Provider based on config

    //Add all commands that need to be available
    rootCmd.AddCommand(cmd.GetJumpCmd()) //TODO doesnt work
    rootCmd.AddCommand(cmd.GetListCmd())

    //Repository commands
    // rootCmd.AddCommand(cmd.GetSyncCmd(provider)) //Get changes or clone
}

func initConfig() {
    if cfgFile != "" {
        viper.SetConfigFile(cfgFile)
    } else {
        home, err := os.UserHomeDir()
        if err != nil {
            fmt.Printf("Error finding home directory: %v\n", err)
            os.Exit(1)
        }

        viper.AddConfigPath(home + ".config/repowatcher/")
        viper.SetConfigName("config.yaml")
    }

    viper.AutomaticEnv()

    if err := viper.ReadInConfig(); err != nil {
        fmt.Printf("Error reading config file: %v\n", err)
        os.Exit(1)
    }
}

func main() {
    if err := rootCmd.Execute(); err != nil {
        fmt.Println(err)
        os.Exit(1)
    }
}

