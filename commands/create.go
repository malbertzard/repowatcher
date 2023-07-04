package commands

import (
	"log"
	"repo-watch/helpers"
	"repo-watch/models"
	"repo-watch/receiver"
)

func CreateRepository(config *models.Config, configFile string, receiver receiver.Receiver) {
	if err := helpers.LoadConfig(configFile, config); err != nil {
		log.Fatalf("Failed to load config file: %v", err)
	}

	repository := createRepository()

	config.Repositories = append(config.Repositories, repository)

	if err := helpers.SaveConfig(configFile, *config); err != nil {
		log.Fatalf("Failed to save config file: %v", err)
	}

	err := createDirectory(&repository, config)
	if err != nil {
		log.Fatalf("Failed to create directory: %v", err)
	}

	err = initializeGitRepository(&repository, config)
	if err != nil {
		log.Fatalf("Failed to initialize Git repository: %v", err)
	}

	log.Printf("Repository added: %s", repository.Nickname)
}

func createRepository() models.Repository {
	nickname := readInput("Nickname: ")
	folderName := readInput("Folder Name: ")
	url := readInput("URL: ")
	sparse := readBoolInput("Enable Sparse Checkout? (y/n): ")

	repository := models.Repository{
		Nickname:   nickname,
		FolderName: folderName,
		URL:        url,
		Sparse:     sparse,
	}

	return repository
}

func createDirectory(repository *models.Repository, config *models.Config) error {
	repoPath := helpers.GetRepositoryPath(repository, config)

	err := helpers.RunCommand("mkdir", "-p", repoPath)
	if err != nil {
		return err
	}

	return nil
}

func initializeGitRepository(repository *models.Repository, config *models.Config) error {
	repoPath := helpers.GetRepositoryPath(repository, config)

	err := helpers.RunCommand("git", "init", repoPath)
	if err != nil {
		return err
	}

	return nil
}
