package main

import (
	"fmt"
	"log"
	"os"
	"repo-watch/commands"
	"repo-watch/helpers"
	"repo-watch/models"

	"github.com/spf13/cobra"
	"gopkg.in/yaml.v2"
)

var (
	configFile    string
	config        models.Config
	allReposFlag  bool
	exampleConfig = `---
rootFolder: /path/to/repositories
editCommand: code
repositories:
  - nickname: repo1
    folderName: repo1
    url: https://github.com/user/repo1.git
    sparse: true
  - nickname: repo2
    folderName: repo2
    url: https://github.com/user/repo2.git
    sparse: false`
)

func main() {
	rootCmd := &cobra.Command{
		Use:   "rw",
		Short: "A tool for managing multiple Git repositories",
		Run:   displayHelp,
	}

	rootCmd.PersistentFlags().StringVarP(&configFile, "config", "c", "config.yaml", "config file")
	rootCmd.PersistentFlags().BoolVarP(&allReposFlag, "all", "a", false, "apply command to all repositories")

	rootCmd.AddCommand(&cobra.Command{
		Use:   "fetch",
		Short: "Fetch changes from remote for one or all repositories",
		Run:   fetchChangesCommand,
	})

	rootCmd.AddCommand(&cobra.Command{
		Use:   "list",
		Short: "List repositories",
		Run:   listRepositoriesCommand,
	})

	rootCmd.AddCommand(&cobra.Command{
		Use:   "pull",
		Short: "Pull changes from remote for one or all repositories",
		Run:   pullChangesCommand,
	})

	rootCmd.AddCommand(&cobra.Command{
		Use:   "clone [nickname]",
		Short: "Clone a repository or all repositories",
		Args:  cobra.MaximumNArgs(1),
		Run:   cloneRepositoryCommand,
	})

	rootCmd.AddCommand(&cobra.Command{
		Use:   "diff [nickname]",
		Short: "Show diff for a repository or all repositories",
		Args:  cobra.MaximumNArgs(1),
		Run:   showRepositoryDiffCommand,
	})

	rootCmd.AddCommand(&cobra.Command{
		Use:   "edit [nickname]",
		Short: "Open a repository in IDE",
		Args:  cobra.ExactArgs(1),
		Run:   openRepositoryInIDECommand,
	})

	rootCmd.AddCommand(&cobra.Command{
		Use:   "exec [nickname] [command]",
		Short: "Execute a command in a repository",
		Args:  cobra.MinimumNArgs(2),
		Run:   executeCommandInRepositoryCommand,
	})

	rootCmd.AddCommand(&cobra.Command{
		Use:   "generate-config",
		Short: "Generate an example config file",
		Run:   generateExampleConfigCommand,
	})

	if err := rootCmd.Execute(); err != nil {
		log.Fatal(err)
	}
}

func listRepositoriesCommand(cmd *cobra.Command, args []string) {
	if err := loadConfig(); err != nil {
		log.Fatal(err)
	}
    commands.ListRepositories(&config)
}

func fetchChangesCommand(cmd *cobra.Command, args []string) {
	if err := loadConfig(); err != nil {
		log.Fatal(err)
	}

    nickname := helpers.GetNicknameFromArgs(args)
    commands.FetchRepositories(&config, nickname, allReposFlag)
}

func pullChangesCommand(cmd *cobra.Command, args []string) {
	if err := loadConfig(); err != nil {
		log.Fatal(err)
	}

    nickname := helpers.GetNicknameFromArgs(args)
    commands.PullRepositories(&config,nickname,allReposFlag)
}

func cloneRepositoryCommand(cmd *cobra.Command, args []string) {
	if err := loadConfig(); err != nil {
		log.Fatal(err)
	}

    nickname := helpers.GetNicknameFromArgs(args)
    commands.CloneRepositories(&config,nickname,allReposFlag)
}


func showRepositoryDiffCommand(cmd *cobra.Command, args []string) {
	if err := loadConfig(); err != nil {
		log.Fatal(err)
	}

    nickname := helpers.GetNicknameFromArgs(args)
    commands.DiffRepositories(&config,nickname,allReposFlag)
}

func openRepositoryInIDECommand(cmd *cobra.Command, args []string) {
	if err := loadConfig(); err != nil {
		log.Fatal(err)
	}

    nickname := helpers.GetNicknameFromArgs(args)
	repo := helpers.FindRepositoryByNickname(nickname, &config)
    commands.OpenideRepositories(repo, &config)
}

func executeCommandInRepositoryCommand(cmd *cobra.Command, args []string) {
	if err := loadConfig(); err != nil {
		log.Fatal(err)
	}

	nickname := args[0]
	commandArgs := args[1:]
	repo := helpers.FindRepositoryByNickname(nickname, &config)
    commands.ExecInRepositories(repo, commandArgs, &config)
}

func generateExampleConfigCommand(cmd *cobra.Command, args []string) {
	fmt.Println(exampleConfig)
}

func displayHelp(cmd *cobra.Command, args []string) {
	cmd.Help()
}

//Helpers

func loadConfig() error {
	file, err := os.Open(configFile)
	if err != nil {
		return fmt.Errorf("failed to open config file: %v", err)
	}
	defer file.Close()

	decoder := yaml.NewDecoder(file)
	if err := decoder.Decode(&config); err != nil {
		return fmt.Errorf("failed to decode config file: %v", err)
	}

	return nil
}
