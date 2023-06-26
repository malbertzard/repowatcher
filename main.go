package main

import (
	"log"
	"repo-watch/commands"
	"repo-watch/helpers"
	"repo-watch/models"
	"repo-watch/receiver"

	"github.com/spf13/cobra"
)

var (
	configFile    string
	config        models.Config
	allReposFlag  bool
	jsonOutput    bool
)

func main() {
	rootCmd := &cobra.Command{
		Use:   "rw",
		Short: "A tool for managing multiple Git repositories",
		Run:   displayHelp,
	}

	rootCmd.PersistentFlags().StringVarP(&configFile, "config", "c", "config.yaml", "config file")
	rootCmd.PersistentFlags().BoolVarP(&allReposFlag, "all", "a", false, "apply command to all repositories")
	rootCmd.PersistentFlags().BoolVar(&jsonOutput, "json", false, "output in JSON format")

	rootCmd.AddCommand(newFetchCommand())
	rootCmd.AddCommand(newListCommand())
	rootCmd.AddCommand(newPullCommand())
	rootCmd.AddCommand(newCloneCommand())
	rootCmd.AddCommand(newDiffCommand())
	rootCmd.AddCommand(newRemoteDiffCommand())
	rootCmd.AddCommand(newEditCommand())
	rootCmd.AddCommand(newExecCommand())

	rootCmd.AddCommand(newAddRepositoryCommand())

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
		Use:   "add-repo",
		Short: "Add a repository to the configuration",
		Run: func(cmd *cobra.Command, args []string) {
			if err := helpers.LoadConfig(configFile, &config); err != nil {
				log.Fatal(err)
			}

			receiver := getReceiver()
			commands.AddRepository(&config, configFile,receiver)
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
			commands.DiffRepositories(&config, nickname, receiver, allReposFlag)
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
			commands.DiffRemoteRepositories(&config, nickname, receiver, allReposFlag)
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
			if err := helpers.LoadConfig(configFile ,&config); err != nil {
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
