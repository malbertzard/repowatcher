package helpers

import (
	"fmt"
	"io/ioutil"
	"os"
	"path/filepath"
	"repo-watch/models"

	"gopkg.in/yaml.v2"
)

func GetRepositoryPath(repo *models.Repository, config *models.Config) string {
	return filepath.Join(config.RootFolder, repo.FolderName)
}

func FindRepositoryByNickname(nickname string, config *models.Config) *models.Repository {
	for _, repo := range config.Repositories {
		if repo.Nickname == nickname {
			return &repo
		}
	}
	return nil
}

func GetNicknameFromArgs(args []string) string {
	nickname := ""
	if len(args) > 0 {
		nickname = args[0]
	}
	return nickname
}

func LoadConfig(configFile string, config *models.Config) error {
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

func SaveConfig(file string, config models.Config) error {
	configFile, err := yaml.Marshal(&config)
	if err != nil {
		return err
	}

	dir := filepath.Dir(file)
	if _, err := os.Stat(dir); os.IsNotExist(err) {
		if err := os.MkdirAll(dir, 0755); err != nil {
			return err
		}
	}

	if err := ioutil.WriteFile(file, configFile, 0644); err != nil {
		return err
	}

	return nil
}
