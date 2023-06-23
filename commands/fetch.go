package commands

import (
	"fmt"
	"os"
	"os/exec"
	"repo-watch/helpers"
	"repo-watch/models"
)

func FetchRepositories(config *models.Config, nickname string, allReposFlag bool)  {
	if allReposFlag {
		for _, repo := range config.Repositories {
			fetchRepository(&repo, config)
		}
	} else {
		repo := helpers.FindRepositoryByNickname(nickname, config)
		if repo != nil {
			fetchRepository(repo, config)
		} else {
			fmt.Println("Repository not found in config.")
		}
	}
    
}

func fetchRepository(repo *models.Repository, config *models.Config) {
	repoPath := helpers.GetRepositoryPath(repo, config)
	cmd := exec.Command("git", "-C", repoPath, "fetch", "--all")
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	err := cmd.Run()
	if err != nil {
		fmt.Printf("Failed to fetch changes for repository %s: %v\n", repo.Nickname, err)
	}
}
