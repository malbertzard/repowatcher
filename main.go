package main

import (
	"log"
	"os"
	"path/filepath"
	"repo-watch/commands"
	"repo-watch/helpers"
	"repo-watch/models"
	"repo-watch/receiver"

	"github.com/spf13/cobra"
)

var (
	configFile   string
	config       models.Config
	allReposFlag bool
	jsonOutput   bool
)

func main() {
	// Get the user's home directory
	homeDir, err := os.UserHomeDir()
	if err != nil {
		log.Fatal(err)
	}

	defaultConfigFile := filepath.Join(homeDir, ".config", "rw", "config.yaml")

	if _, err := os.Stat(defaultConfigFile); os.IsNotExist(err) {
		configDir := filepath.Dir(defaultConfigFile)
		if err := os.MkdirAll(configDir, 0700); err != nil {
			log.Fatal(err)
		}
	}

	// Use the default config file if the configFile flag is not set
	if configFile == "" {
		configFile = defaultConfigFile
	}

	rootCmd := &cobra.Command{
		Use:   "rw",
		Short: "A tool for managing multiple Git repositories",
		Run:   displayHelp,
	}

	rootCmd.PersistentFlags().StringVarP(&configFile, "config", "c", configFile, "config file")
	rootCmd.PersistentFlags().BoolVarP(&allReposFlag, "all", "a", false, "apply command to all repositories")
	rootCmd.PersistentFlags().BoolVar(&jsonOutput, "json", false, "output in JSON format")

	rootCmd.AddCommand(newAddRepositoryCommand())
	rootCmd.AddCommand(newListCommand())

	rootCmd.AddCommand(newCloneCommand())
	rootCmd.AddCommand(newFetchCommand())
	rootCmd.AddCommand(newPullCommand())

	rootCmd.AddCommand(newDiffCommand())
	rootCmd.AddCommand(newRemoteDiffCommand())

	rootCmd.AddCommand(newEditCommand())
	rootCmd.AddCommand(newExecCommand())

	if err := rootCmd.Execute(); err != nil {
		log.Fatal(err)
	}
}

func newListCommand() *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List repositories",
		Run: func(cmd *cobra.Command, args []string) {
			if err := helpers.LoadConfig(configFile, &config); err != nil {
				log.Fatal(err)
			}
			receiver := getReceiver()
			commands.ListRepositories(&config, &receiver)
		},
	}
}

func newAddRepositoryCommand() *cobra.Command {
	return &cobra.Command{
		Use:   "add",
		Short: "Add a repository to the configuration",
		Run: func(cmd *cobra.Command, args []string) {
			if err := helpers.LoadConfig(configFile, &config); err != nil {
				log.Fatal(err)
			}

			receiver := getReceiver()
			commands.AddRepository(&config, configFile, receiver)
		},
	}
}

func newFetchCommand() *cobra.Command {
	return &cobra.Command{
		Use:   "fetch",
		Short: "Fetch changes from remote for one or all repositories",
		Run: func(cmd *cobra.Command, args []string) {
			if err := helpers.LoadConfig(configFile, &config); err != nil {
				log.Fatal(err)
			}

			nickname := helpers.GetNicknameFromArgs(args)
			receiver := getReceiver()
			commands.FetchRepositories(&config, nickname, &receiver, allReposFlag)
		},
	}
}

func newPullCommand() *cobra.Command {
	return &cobra.Command{
		Use:   "pull",
		Short: "Pull changes from remote for one or all repositories",
		Run: func(cmd *cobra.Command, args []string) {
			if err := helpers.LoadConfig(configFile, &config); err != nil {
				log.Fatal(err)
			}

			nickname := helpers.GetNicknameFromArgs(args)
			receiver := getReceiver()
			commands.PullRepositories(&config, nickname, &receiver, allReposFlag)
		},
	}
}

func newCloneCommand() *cobra.Command {
	return &cobra.Command{
		Use:   "clone [nickname]",
		Short: "Clone a repository or all repositories",
		Args:  cobra.MaximumNArgs(1),
		Run: func(cmd *cobra.Command, args []string) {
			if err := helpers.LoadConfig(configFile, &config); err != nil {
				log.Fatal(err)
			}

			nickname := helpers.GetNicknameFromArgs(args)
			receiver := getReceiver()
			commands.CloneRepositories(&config, nickname, receiver, allReposFlag)
		},
	}
}

func newDiffCommand() *cobra.Command {
	return &cobra.Command{
		Use:   "diff [nickname]",
		Short: "Show diff for a repository or all repositories",
		Args:  cobra.MaximumNArgs(1),
		Run: func(cmd *cobra.Command, args []string) {
			if err := helpers.LoadConfig(configFile, &config); err != nil {
				log.Fatal(err)
			}

			nickname := helpers.GetNicknameFromArgs(args)
			receiver := getReceiver()
			commands.ShowDiff(&config, nickname, receiver, allReposFlag, false)
		},
	}
}

func newRemoteDiffCommand() *cobra.Command {
	return &cobra.Command{
		Use:   "rdiff [nickname]",
		Short: "Show diff for a repository from remote",
		Args:  cobra.MaximumNArgs(1),
		Run: func(cmd *cobra.Command, args []string) {
			if err := helpers.LoadConfig(configFile, &config); err != nil {
				log.Fatal(err)
			}

			nickname := helpers.GetNicknameFromArgs(args)
			receiver := getReceiver()
			commands.ShowDiff(&config, nickname, receiver, allReposFlag, true)
		},
	}
}

func newEditCommand() *cobra.Command {
	return &cobra.Command{
		Use:   "edit [nickname]",
		Short: "Open a repository in IDE",
		Args:  cobra.ExactArgs(1),
		Run: func(cmd *cobra.Command, args []string) {
			if err := helpers.LoadConfig(configFile, &config); err != nil {
				log.Fatal(err)
			}

			nickname := helpers.GetNicknameFromArgs(args)
			repo := helpers.FindRepositoryByNickname(nickname, &config)
			receiver := getReceiver()
			commands.OpenIDERepositories(repo, &config, receiver)
		},
	}
}

func newExecCommand() *cobra.Command {
	return &cobra.Command{
		Use:   "exec [nickname] [command]",
		Short: "Execute a command in a repository",
		Args:  cobra.MinimumNArgs(2),
		Run: func(cmd *cobra.Command, args []string) {
			if err := helpers.LoadConfig(configFile, &config); err != nil {
				log.Fatal(err)
			}

			nickname := args[0]
			commandArgs := args[1:]
			repo := helpers.FindRepositoryByNickname(nickname, &config)
			receiver := getReceiver()
			commands.ExecInRepositories(repo, commandArgs, &config, receiver)
		},
	}
}

func displayHelp(cmd *cobra.Command, args []string) {
	cmd.Help()
}

func getReceiver() receiver.Receiver {
	if jsonOutput {
		return receiver.NewJSONReceiver()
	}
	return receiver.NewTextReceiver()
}
